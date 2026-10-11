//! Ingress port gate for all-port forwarding: hold the first packets to a cold port until its
//! listener exists, instead of losing them.
//!
//! smoltcp answers a SYN to a port with no listener with an RST (and an unmatched UDP datagram
//! with ICMP port-unreachable) inside its ingress loop, so a per-port listener must exist
//! *before* the netstack sees the flow's first packet. The raw-socket observer in `all_port`
//! learns the port only after the netstack has swallowed that packet, so every new destination
//! port costs the client a SYN retransmit (~1 s) or a lost DNS query / QUIC Initial, and the
//! observer copies every inbound packet to do it.
//!
//! [`PortGate`] sits in front of the forwarder netstack's ingress queue instead. Packets to an
//! open port pass straight through; a connection-initiating packet (TCP SYN, or any UDP datagram)
//! to a closed port is held and the manager is asked to open the port. Once the listener is
//! registered the held packets are released in order, ahead of anything that arrives later, and
//! the port is open. Nothing is copied on the hot path and no raw socket is needed.
//!
//! Everything else passes unchanged: non-IPv4 packets, IPv4 fragments (smoltcp reassembles them;
//! a fragmented *first* datagram to a cold UDP port is the one case still answered with
//! port-unreachable), and non-SYN TCP to a closed port (smoltcp RSTs it, which is correct for a
//! flow this netstack doesn't know).
//!
//! The gate never dials anything and does not classify routes; every flow still goes through the
//! per-port listener's classify → [`RealDialer`](crate::RealDialer) chokepoint.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ts_netstack_smoltcp::{
    WakingPipeSender,
    netcore::smoltcp::wire::{IpProtocol, Ipv4Packet, TcpPacket, UdpPacket},
};

/// Transport protocol of a gated port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Proto {
    Tcp,
    Udp,
}

/// Max packets held for one port while its listener is being opened. A browser opening a page
/// sends a handful of parallel SYNs to the same cold port; far more than this is a flood.
const MAX_HELD_PER_PORT: usize = 32;

/// Max bytes held across all opening ports. Bounds what a port sweep can pin in the gate.
const MAX_HELD_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Closed,
    Opening,
    Open,
}

#[derive(Clone, Copy)]
struct Slot {
    state: State,
    /// Last packet seen for this port, in seconds since the gate's epoch.
    last: u32,
}

struct Inner {
    ingress: WakingPipeSender,
    epoch: Instant,
    tcp: Vec<Slot>,
    udp: Vec<Slot>,
    held: HashMap<(Proto, u16), Vec<Vec<u8>>>,
    held_bytes: usize,
    /// Ports that are opening or open, across both protocols.
    active: usize,
    max_active: usize,
    open_requests: tokio::sync::mpsc::UnboundedSender<(Proto, u16)>,
}

/// What the gate did with a packet.
#[derive(Debug, PartialEq, Eq)]
pub enum Admit {
    /// Handed to the netstack's ingress queue.
    Passed,
    /// Held until its port's listener is up.
    Held,
    /// Dropped: the ingress queue was full, or the hold budget / active-port cap was reached.
    Dropped,
}

/// Ingress gate in front of an all-port forwarder netstack. See the module docs.
///
/// Cheap to clone; all clones share one state.
#[derive(Clone)]
pub struct PortGate {
    inner: Arc<Mutex<Inner>>,
}

/// The manager side of a [`PortGate`]: open requests to serve.
pub(crate) struct OpenRequests(pub(crate) tokio::sync::mpsc::UnboundedReceiver<(Proto, u16)>);

impl PortGate {
    /// A gate feeding `ingress` (the forwarder netstack's ingress queue), allowing at most
    /// `max_active` ports (TCP and UDP together) to be opening or open at once.
    pub(crate) fn new(ingress: WakingPipeSender, max_active: usize) -> (Self, OpenRequests) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let closed = Slot {
            state: State::Closed,
            last: 0,
        };
        let inner = Inner {
            ingress,
            epoch: Instant::now(),
            tcp: vec![closed; 1 << 16],
            udp: vec![closed; 1 << 16],
            held: HashMap::new(),
            held_bytes: 0,
            active: 0,
            max_active,
            open_requests: tx,
        };
        (
            Self {
                inner: Arc::new(Mutex::new(inner)),
            },
            OpenRequests(rx),
        )
    }

    /// Offer one inbound packet to the forwarder netstack.
    pub fn ingress(&self, packet: &[u8]) -> Admit {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        let Some((proto, port, initiating)) = classify(packet) else {
            return inner.pass(packet);
        };
        let now = inner.now();
        let slot = &mut inner.slots(proto)[port as usize];
        match slot.state {
            State::Open => {
                slot.last = now;
                inner.pass(packet)
            }
            State::Opening => inner.hold(proto, port, packet),
            State::Closed if !initiating => inner.pass(packet),
            State::Closed => {
                if inner.active >= inner.max_active {
                    tracing::debug!(?proto, %port, "port gate: at max active ports; dropping");
                    return Admit::Dropped;
                }
                if inner.held_bytes + packet.len() > MAX_HELD_BYTES {
                    return Admit::Dropped;
                }
                if inner.open_requests.send((proto, port)).is_err() {
                    // Manager gone: nothing will ever open the port.
                    return Admit::Dropped;
                }
                inner.active += 1;
                let slot = &mut inner.slots(proto)[port as usize];
                slot.state = State::Opening;
                slot.last = now;
                inner.hold(proto, port, packet)
            }
        }
    }

    /// The listener for `port` is registered: release its held packets, then open the port.
    ///
    /// Releasing under the lock keeps ordering: anything arriving meanwhile waits and is passed
    /// after the held packets.
    pub(crate) fn mark_open(&self, proto: Proto, port: u16) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.slots(proto)[port as usize].state != State::Opening {
            return;
        }
        let held = inner.held.remove(&(proto, port)).unwrap_or_default();
        for p in &held {
            inner.held_bytes -= p.len();
            inner.ingress.try_send(p);
        }
        inner.slots(proto)[port as usize].state = State::Open;
        tracing::debug!(?proto, %port, released = held.len(), "port gate: open");
    }

    /// The listener for `port` is gone (failed, evicted or exited): close the port and drop
    /// anything still held for it, so the next initiating packet re-opens it.
    pub(crate) fn close(&self, proto: Proto, port: u16) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.slots(proto)[port as usize].state == State::Closed {
            return;
        }
        if let Some(held) = inner.held.remove(&(proto, port)) {
            inner.held_bytes -= held.iter().map(Vec::len).sum::<usize>();
        }
        inner.slots(proto)[port as usize].state = State::Closed;
        inner.active -= 1;
    }

    /// Open ports that have seen no packet for at least `idle`.
    pub(crate) fn idle_ports(&self, idle: Duration) -> Vec<(Proto, u16)> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now = inner.now();
        let idle = u32::try_from(idle.as_secs()).unwrap_or(u32::MAX);
        let mut out = Vec::new();
        for (proto, slots) in [(Proto::Tcp, &inner.tcp), (Proto::Udp, &inner.udp)] {
            for (port, s) in slots.iter().enumerate() {
                if s.state == State::Open && now.saturating_sub(s.last) >= idle {
                    out.push((proto, port as u16));
                }
            }
        }
        out
    }
}

impl Inner {
    fn now(&self) -> u32 {
        u32::try_from(self.epoch.elapsed().as_secs()).unwrap_or(u32::MAX)
    }

    fn slots(&mut self, proto: Proto) -> &mut [Slot] {
        match proto {
            Proto::Tcp => &mut self.tcp,
            Proto::Udp => &mut self.udp,
        }
    }

    fn pass(&self, packet: &[u8]) -> Admit {
        if self.ingress.try_send(packet) {
            Admit::Passed
        } else {
            Admit::Dropped
        }
    }

    fn hold(&mut self, proto: Proto, port: u16, packet: &[u8]) -> Admit {
        let budget_left = MAX_HELD_BYTES - self.held_bytes;
        let held = self.held.entry((proto, port)).or_default();
        if held.len() >= MAX_HELD_PER_PORT || packet.len() > budget_left {
            return Admit::Dropped;
        }
        held.push(packet.to_vec());
        self.held_bytes += packet.len();
        Admit::Held
    }
}

/// `(protocol, destination port, opens a flow)` for an unfragmented IPv4 TCP/UDP packet; `None`
/// for anything the gate passes through untouched.
fn classify(packet: &[u8]) -> Option<(Proto, u16, bool)> {
    let ip = Ipv4Packet::new_checked(packet).ok()?;
    if ip.more_frags() || ip.frag_offset() != 0 {
        return None;
    }
    match ip.next_header() {
        IpProtocol::Tcp => {
            let tcp = TcpPacket::new_checked(ip.payload()).ok()?;
            Some((Proto::Tcp, tcp.dst_port(), tcp.syn() && !tcp.ack()))
        }
        IpProtocol::Udp => {
            let udp = UdpPacket::new_checked(ip.payload()).ok()?;
            Some((Proto::Udp, udp.dst_port(), true))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ts_netstack_smoltcp::{WakingPipe, netcore::smoltcp::wire::Ipv4Address};

    use super::*;

    fn ipv4(proto: IpProtocol, payload: &[u8], frag_offset: u16) -> Vec<u8> {
        let total = 20 + payload.len();
        let mut buf = vec![0u8; total];
        let mut ip = Ipv4Packet::new_unchecked(&mut buf);
        ip.set_version(4);
        ip.set_header_len(20);
        ip.set_total_len(total as u16);
        ip.set_hop_limit(64);
        ip.set_next_header(proto);
        ip.set_frag_offset(frag_offset);
        ip.set_src_addr(Ipv4Address::new(100, 64, 0, 1));
        ip.set_dst_addr(Ipv4Address::new(1, 1, 1, 1));
        ip.payload_mut().copy_from_slice(payload);
        buf
    }

    fn tcp(dst_port: u16, syn: bool, ack: bool) -> Vec<u8> {
        let mut seg = vec![0u8; 20];
        let mut tcp = TcpPacket::new_unchecked(&mut seg);
        tcp.set_src_port(40000);
        tcp.set_dst_port(dst_port);
        tcp.set_header_len(20);
        tcp.set_syn(syn);
        tcp.set_ack(ack);
        ipv4(IpProtocol::Tcp, &seg, 0)
    }

    fn udp(dst_port: u16) -> Vec<u8> {
        let mut dg = vec![0u8; 8];
        let mut udp = UdpPacket::new_unchecked(&mut dg);
        udp.set_src_port(40000);
        udp.set_dst_port(dst_port);
        udp.set_len(8);
        ipv4(IpProtocol::Udp, &dg, 0)
    }

    fn gate(max_active: usize) -> (PortGate, OpenRequests, WakingPipe) {
        let (ours, netstack_side) = WakingPipe::unbounded();
        let (g, reqs) = PortGate::new(ours.tx, max_active);
        (g, reqs, netstack_side)
    }

    fn drain(rx: &mut WakingPipe) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| rx.rx.try_recv().map(|b| b.to_vec())).collect()
    }

    #[test]
    fn cold_syn_is_held_then_released_in_order() {
        let (g, mut reqs, mut ns) = gate(16);
        let syn1 = tcp(443, true, false);
        let syn2 = tcp(443, true, false);

        assert_eq!(g.ingress(&syn1), Admit::Held);
        assert_eq!(g.ingress(&syn2), Admit::Held);
        // Exactly one open request for the port, however many SYNs raced it.
        assert_eq!(reqs.0.try_recv().unwrap(), (Proto::Tcp, 443));
        assert!(reqs.0.try_recv().is_err());
        assert!(drain(&mut ns).is_empty());

        g.mark_open(Proto::Tcp, 443);
        assert_eq!(drain(&mut ns), vec![syn1, syn2]);

        let data = tcp(443, false, true);
        assert_eq!(g.ingress(&data), Admit::Passed);
        assert_eq!(drain(&mut ns), vec![data]);
    }

    #[test]
    fn cold_udp_is_held_then_released() {
        let (g, mut reqs, mut ns) = gate(16);
        let q = udp(53);
        assert_eq!(g.ingress(&q), Admit::Held);
        assert_eq!(reqs.0.try_recv().unwrap(), (Proto::Udp, 53));
        g.mark_open(Proto::Udp, 53);
        assert_eq!(drain(&mut ns), vec![q]);
    }

    #[test]
    fn non_syn_and_non_ipv4_pass_through_closed_ports() {
        let (g, mut reqs, mut ns) = gate(16);
        let ack = tcp(443, false, true);
        assert_eq!(g.ingress(&ack), Admit::Passed);
        let v6 = {
            let mut p = vec![0u8; 40];
            p[0] = 0x60;
            p
        };
        assert_eq!(g.ingress(&v6), Admit::Passed);
        let mut frag = udp(53);
        Ipv4Packet::new_unchecked(&mut frag).set_frag_offset(8);
        assert_eq!(g.ingress(&frag), Admit::Passed);
        assert!(reqs.0.try_recv().is_err());
        assert_eq!(drain(&mut ns).len(), 3);
    }

    #[test]
    fn close_drops_held_and_reopens_on_next_syn() {
        let (g, mut reqs, mut ns) = gate(16);
        assert_eq!(g.ingress(&tcp(8080, true, false)), Admit::Held);
        let _ = reqs.0.try_recv();
        g.close(Proto::Tcp, 8080);
        // A late mark_open for a closed port releases nothing.
        g.mark_open(Proto::Tcp, 8080);
        assert!(drain(&mut ns).is_empty());
        assert_eq!(g.ingress(&tcp(8080, true, false)), Admit::Held);
        assert_eq!(reqs.0.try_recv().unwrap(), (Proto::Tcp, 8080));
    }

    #[test]
    fn caps_bound_active_ports_and_held_packets() {
        let (g, mut reqs, _ns) = gate(2);
        assert_eq!(g.ingress(&tcp(1, true, false)), Admit::Held);
        assert_eq!(g.ingress(&udp(2)), Admit::Held);
        assert_eq!(g.ingress(&tcp(3, true, false)), Admit::Dropped);
        assert_eq!(std::iter::from_fn(|| reqs.0.try_recv().ok()).count(), 2);

        for _ in 1..MAX_HELD_PER_PORT {
            assert_eq!(g.ingress(&tcp(1, true, false)), Admit::Held);
        }
        assert_eq!(g.ingress(&tcp(1, true, false)), Admit::Dropped);

        // Closing a port frees its active slot.
        g.close(Proto::Tcp, 1);
        assert_eq!(g.ingress(&tcp(3, true, false)), Admit::Held);
    }

    #[test]
    fn idle_ports_reports_only_open_ports() {
        let (g, _reqs, _ns) = gate(16);
        let _ = g.ingress(&tcp(443, true, false));
        let _ = g.ingress(&udp(53));
        g.mark_open(Proto::Tcp, 443);
        assert_eq!(g.idle_ports(Duration::ZERO), vec![(Proto::Tcp, 443)]);
        assert!(g.idle_ports(Duration::from_secs(60)).is_empty());
    }
}
