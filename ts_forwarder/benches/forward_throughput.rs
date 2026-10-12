//! Exit-node forwarder throughput, isolated from WireGuard and the underlay.
//!
//! A "peer" netstack dials an exit-node destination over an in-memory pipe (overlay MTU 1280); the
//! forwarder netstack accepts it, classifies it as `0.0.0.0/0`, dials a real loopback socket through
//! `HostExitDialer`'s socket path and splices. Measures bulk TCP in each direction across N parallel flows.
//!
//! `RTT_MS` inserts a symmetric one-way delay of `RTT_MS / 2` on the overlay pipe, standing in for
//! the peer's path latency (the forwarder's own egress leg stays on loopback).
//!
//! `LOSS_PCT` drops that percentage of overlay packets in each direction. `BW_MBPS` adds a bottleneck
//! link of that rate in each direction, fed by a `QUEUE_KB` tail-drop queue (default 128 KiB).
//!
//! `FWD_MODE` picks how the forwarder takes ports: `ports` (default; an explicit listener on the
//! server's port), `raw` (all-port, raw-socket observer) or `gated` (all-port behind the ingress
//! `PortGate`, what the runtime runs). `TCP_BUF_KB` overrides the per-direction netstack TCP buffer (both stacks).
//!
//! `VERIFY_MB` makes this a bounded finite-transfer regression (fails if the byte target is
//! not reached within `secs`). Normal measurements also stop at `secs`, including stalled I/O.
//!
//! ```sh
//! RTT_MS=40 cargo bench -p geiserx_ts_forwarder --bench forward_throughput -- [secs] [flows] [up|down]
//! ```

use core::net::{Ipv4Addr, SocketAddr};
use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ts_forwarder::{
    DialError, DialedUdp, FlowClass, Forwarder, HostExitDialer, RealDialer, RouteTable,
};
use ts_netstack_smoltcp::{
    CreateSocket, Netstack, WakingPipe, WakingPipeDev, WakingPipeReceiver,
    netcore::{self, Channel, HasChannel, NetstackControl, smoltcp},
};

const PEER_IP: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 1);
const OVERLAY_MTU: usize = 1280;
const CHUNK: usize = 64 * 1024;

/// Bytes the loopback server actually received (`up`), to cross-check the sender-side count.
static SERVER_RX: AtomicU64 = AtomicU64::new(0);
static QUEUE_DROPS: AtomicU64 = AtomicU64::new(0);

/// Opt-in wire trace: ports and TCP sequence space only, never addresses or payloads.
fn trace_packet(pkt: &[u8], seed: u64, dropped: bool) {
    static TRACE: OnceLock<Option<Mutex<(std::fs::File, Instant)>>> = OnceLock::new();
    let trace = TRACE.get_or_init(|| {
        std::env::var("TRACE_TCP")
            .ok()
            .map(|path| Mutex::new((std::fs::File::create(path).unwrap(), Instant::now())))
    });
    let Some(trace) = trace else { return };
    use smoltcp::wire::{Ipv4Packet, TcpPacket, TcpRepr};
    let Ok(ip) = Ipv4Packet::new_checked(pkt) else {
        return;
    };
    let Ok(tcp) = TcpPacket::new_checked(ip.payload()) else {
        return;
    };
    let Ok(repr) = TcpRepr::parse(
        &tcp,
        &ip.src_addr().into(),
        &ip.dst_addr().into(),
        &smoltcp::phy::ChecksumCapabilities::ignored(),
    ) else {
        return;
    };
    use std::io::Write;
    let mut guard = trace.lock().unwrap();
    let ms = guard.1.elapsed().as_secs_f64() * 1000.0;
    writeln!(
        guard.0,
        "{ms:.3} {seed:x} drop={dropped} port={} seq={} ack={:?} len={} win={} sack={:?}",
        repr.src_port,
        repr.seq_number,
        repr.ack_number,
        repr.payload.len(),
        repr.window_len,
        repr.sack_ranges
    )
    .unwrap();
}

/// Relay packets from `rx` to `deliver`, each delivered `delay` after it was received.
fn spawn_relay(
    mut rx: WakingPipeReceiver,
    deliver: impl Fn(&[u8]) + Send + 'static,
    delay: Duration,
    loss: f64,
    seed: u64,
) {
    // Drop each packet with probability `loss` (xorshift; no extra dependency).
    let mut state = seed | 1;
    let mut lose = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64 <= loss
    };
    // Optional bottleneck: a `BW_MBPS` link fed by a `QUEUE_KB` tail-drop queue (default 128 KiB).
    let bw_bps: f64 = std::env::var("BW_MBPS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(0.0, |m| m * 1e6);
    let queue_bytes: f64 = std::env::var("QUEUE_KB")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(128.0)
        * 1024.0;
    let (q_tx, mut q_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut link_free = tokio::time::Instant::now();
        while let Some(pkt) = rx.recv_async().await {
            if loss > 0.0 && lose() {
                trace_packet(&pkt, seed, true);
                continue;
            }
            let now = tokio::time::Instant::now();
            let mut due = now + delay;
            if bw_bps > 0.0 {
                let backlog = link_free.saturating_duration_since(now).as_secs_f64() * bw_bps / 8.0;
                if backlog + pkt.len() as f64 > queue_bytes {
                    QUEUE_DROPS.fetch_add(1, Ordering::Relaxed);
                    trace_packet(&pkt, seed, true);
                    continue;
                }
                link_free =
                    link_free.max(now) + Duration::from_secs_f64(pkt.len() as f64 * 8.0 / bw_bps);
                due = link_free + delay;
            }
            trace_packet(&pkt, seed, false);
            if q_tx.send((due, pkt)).is_err() {
                return;
            }
        }
    });
    tokio::spawn(async move {
        while let Some((due, pkt)) = q_rx.recv().await {
            if !delay.is_zero() || bw_bps > 0.0 {
                tokio::time::sleep_until(due).await;
            }
            deliver(&pkt);
        }
    });
}

/// `HostExitDialer` refuses loopback for exit flows; dial the loopback server as a subnet flow
/// instead (same socket path).
struct LoopbackInternetDialer;

impl RealDialer for LoopbackInternetDialer {
    async fn dial_tcp(
        &self,
        _class: FlowClass,
        dst: SocketAddr,
    ) -> Result<tokio::net::TcpStream, DialError> {
        HostExitDialer.dial_tcp(FlowClass::Subnet, dst).await
    }

    async fn dial_udp(&self, _class: FlowClass, dst: SocketAddr) -> Result<DialedUdp, DialError> {
        HostExitDialer.dial_udp(FlowClass::Subnet, dst).await
    }
}

/// How the forwarder takes ports.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// An explicit listener on the server's port.
    Ports,
    /// All-port, raw-socket observer.
    Raw,
    /// All-port, ingress port gate (what the runtime uses).
    Gated,
}

/// Peer netstack <-> relay (delay, and the gate in `Gated` mode) <-> forwarder. Returns the peer's
/// channel.
async fn spawn_forwarder(
    rtt: Duration,
    loss: f64,
    tcp_buf: Option<usize>,
    mode: Mode,
    ports: Vec<u16>,
) -> Channel {
    let mut config = netcore::Config {
        mtu: OVERLAY_MTU,
        ..netcore::Config::default()
    };
    if let Some(b) = tcp_buf {
        config.tcp_buffer_size = b;
    }
    let (p1, a) = WakingPipe::new(None);
    let (b, p2) = WakingPipe::new(None);
    let dev = |pipe| WakingPipeDev {
        pipe,
        mtu: OVERLAY_MTU,
        medium: smoltcp::phy::Medium::Ip,
    };
    let mut peer = Netstack::new(dev(p1), config.clone());
    let mut fwd = Netstack::new(dev(p2), config);
    if let Ok(budget) = std::env::var("NETSTACK_WORK_BUDGET") {
        let budget: usize = budget.parse().expect("NETSTACK_WORK_BUDGET is an integer");
        peer.set_work_budget((budget > 0).then_some(budget));
        fwd.set_work_budget((budget > 0).then_some(budget));
    }
    let peer_ch = peer.command_channel();
    let fwd_ch = fwd.command_channel();
    tokio::spawn(async move { peer.run_tokio().await });
    tokio::spawn(async move { fwd.run_tokio().await });
    peer_ch.set_ips([PEER_IP.into()]).await.unwrap();
    fwd_ch.set_any_ip(true).await.unwrap();

    let routes = RouteTable::new(["0.0.0.0/0".parse().unwrap()]);
    let into_fwd = b.tx.clone();
    let (forwarder, gate) = match mode {
        Mode::Ports => (
            Forwarder::new(fwd_ch, routes, LoopbackInternetDialer, ports, vec![]),
            None,
        ),
        Mode::Raw => (
            Forwarder::all_ports(fwd_ch, routes, LoopbackInternetDialer),
            None,
        ),
        Mode::Gated => {
            let (f, g) = Forwarder::all_ports_gated(
                fwd_ch,
                routes,
                LoopbackInternetDialer,
                into_fwd.clone(),
            );
            (f, Some(g))
        }
    };
    tokio::spawn(async move {
        let _ = forwarder.run().await;
    });

    let half = rtt / 2;
    match gate {
        Some(gate) => spawn_relay(
            a.rx,
            move |p| {
                let _ = gate.ingress(p);
            },
            half,
            loss,
            0x9e37_79b9,
        ),
        None => spawn_relay(
            a.rx,
            move |p| {
                let _ = into_fwd.try_send(p);
            },
            half,
            loss,
            0x9e37_79b9,
        ),
    }
    let to_peer = a.tx;
    spawn_relay(
        b.rx,
        move |p| {
            let _ = to_peer.try_send(p);
        },
        half,
        loss,
        0x85eb_ca6b,
    );
    peer_ch
}

/// Loopback server: in `up` mode it discards, in `down` mode it streams zeros.
async fn spawn_server(down: bool) -> SocketAddr {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = vec![0u8; CHUNK];
                loop {
                    let r = if down {
                        s.write_all(&buf).await.map(|_| 1)
                    } else {
                        s.read(&mut buf).await
                    };
                    match r {
                        Ok(n) if n > 0 => {
                            if !down {
                                SERVER_RX.fetch_add(n as u64, Ordering::Relaxed);
                            }
                        }
                        _ => return,
                    }
                }
            });
        }
    });
    addr
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    let secs: f64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(5.0);
    let flows: u16 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let down = args.get(2).is_some_and(|a| a == "down");

    let rtt_ms: u64 = std::env::var("RTT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let tcp_buf_kb: Option<usize> = std::env::var("TCP_BUF_KB")
        .ok()
        .and_then(|v| v.parse().ok());

    let mode = match std::env::var("FWD_MODE").as_deref() {
        Ok("raw") => Mode::Raw,
        Ok("gated") => Mode::Gated,
        _ => Mode::Ports,
    };

    let loss_pct: f64 = std::env::var("LOSS_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    let server = spawn_server(down).await;
    let probe_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let probe_addr = probe_server.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = probe_server.accept().await {
            tokio::spawn(async move {
                let mut byte = [0];
                while s.read_exact(&mut byte).await.is_ok() {
                    if s.write_all(&byte).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    let peer_ch = spawn_forwarder(
        Duration::from_millis(rtt_ms),
        loss_pct / 100.0,
        tcp_buf_kb.map(|k| k * 1024),
        mode,
        vec![server.port(), probe_addr.port()],
    )
    .await;

    // The first connect goes to a cold port: in `raw` mode its SYN is lost and the connect waits
    // for a retransmit; in `gated` mode it is held for the new listener.
    let started = Instant::now();
    let first = SocketAddr::new(PEER_IP.into(), 9_999);
    drop(
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match peer_ch.tcp_connect(first, server).await {
                    Ok(s) => break s,
                    Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            }
        })
        .await
        .expect("cold connect exceeded 10 seconds"),
    );
    let cold_connect_ms = started.elapsed().as_secs_f64() * 1e3;

    SERVER_RX.store(0, Ordering::Relaxed);
    let verify_bytes: Option<u64> = std::env::var("VERIFY_MB")
        .ok()
        .map(|v| v.parse::<u64>().expect("VERIFY_MB must be an integer") * 1024 * 1024);
    let start = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(secs);
    let probe = if std::env::var_os("LATENCY").is_some() {
        let ch = peer_ch.clone();
        Some(tokio::spawn(async move {
            let mut samples = Vec::new();
            let mut failures = 0;
            let mut probe_port = 20_000;
            while tokio::time::Instant::now() < deadline {
                let t = Instant::now();
                let result = tokio::time::timeout_at(
                    deadline.min(tokio::time::Instant::now() + Duration::from_secs(3)),
                    async {
                        let mut s = ch
                            .tcp_connect(SocketAddr::new(PEER_IP.into(), probe_port), probe_addr)
                            .await?;
                        s.write_all(&[1]).await?;
                        s.read_exact(&mut [0]).await?;
                        Ok::<_, std::io::Error>(())
                    },
                )
                .await;
                if matches!(result, Ok(Ok(()))) {
                    samples.push(t.elapsed().as_secs_f64() * 1e3);
                } else if tokio::time::Instant::now() < deadline {
                    failures += 1;
                }
                probe_port += 1;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            samples.sort_by(f64::total_cmp);
            let p95 = samples
                .get((samples.len() * 95).div_ceil(100).saturating_sub(1))
                .copied()
                .unwrap_or(0.0);
            (
                p95,
                samples.last().copied().unwrap_or(0.0),
                samples.len(),
                failures,
            )
        }))
    } else {
        None
    };
    let mut tasks = Vec::new();
    for i in 0..flows {
        let peer_ch = peer_ch.clone();
        tasks.push(tokio::spawn(async move {
            let local = SocketAddr::new(PEER_IP.into(), 10_000 + i);
            let mut s = tokio::time::timeout_at(deadline, async {
                loop {
                    match peer_ch.tcp_connect(local, server).await {
                        Ok(s) => break s,
                        Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
                    }
                }
            })
            .await
            .expect("flow connect exceeded measurement deadline");
            let mut buf = vec![0u8; CHUNK];
            let mut bytes = 0u64;
            let mut last_progress = Instant::now();
            let mut max_idle = 0.0_f64;
            while verify_bytes.is_none_or(|target| bytes < target) {
                let transfer = async {
                    if down {
                        s.read(&mut buf).await
                    } else {
                        // Count partial writes too; cancellation of write_all loses that count.
                        s.write(&buf).await
                    }
                };
                match tokio::time::timeout_at(deadline, transfer).await {
                    Ok(Ok(n)) if n > 0 => {
                        bytes += n as u64;
                        max_idle = max_idle.max(last_progress.elapsed().as_secs_f64());
                        last_progress = Instant::now();
                    }
                    Ok(Ok(_)) | Err(_) => break,
                    Ok(Err(e)) => panic!("transfer failed: {e}"),
                }
            }
            if let Some(target) = verify_bytes {
                assert!(
                    bytes >= target,
                    "transfer stalled: {bytes}/{target} bytes; no progress for {:?}",
                    last_progress.elapsed()
                );
            }
            (bytes, max_idle.max(last_progress.elapsed().as_secs_f64()))
        }));
    }
    let mut total = 0u64;
    let mut stalled_secs = 0.0_f64;
    for t in tasks {
        let (bytes, idle) = t.await.unwrap();
        total += bytes;
        stalled_secs = stalled_secs.max(idle);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let (latency_p95_ms, latency_max_ms, probe_samples, probe_failures) = match probe {
        Some(p) => p.await.unwrap(),
        None => (0.0, 0.0, 0, 0),
    };
    let queue_drops = QUEUE_DROPS.load(Ordering::Relaxed);
    let mbps = |b: u64| b as f64 * 8.0 / elapsed / 1e6;
    println!(
        "dir={} mode={mode:?} rtt_ms={rtt_ms} loss_pct={loss_pct} tcp_buf_kb={} flows={flows} secs={elapsed:.2} mbps={:.1} server_rx_mbps={:.1} cold_connect_ms={cold_connect_ms:.0} stalled_secs={stalled_secs:.2} queue_drops={queue_drops} latency_p95_ms={latency_p95_ms:.1} latency_max_ms={latency_max_ms:.1} probe_samples={probe_samples} probe_failures={probe_failures}",
        if down { "down" } else { "up" },
        tcp_buf_kb.map_or("default".to_owned(), |k| k.to_string()),
        mbps(total),
        mbps(SERVER_RX.load(Ordering::Relaxed)),
    );
}
