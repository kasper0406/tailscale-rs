//! Counters gated by TS_DEBUG_PIPELINE_METRICS=1; no packet-path formatting.
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};
static TURNS: [AtomicU64; 65] = [const { AtomicU64::new(0) }; 65];
static PUBLICATIONS: AtomicU64 = AtomicU64::new(0);
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TS_DEBUG_PIPELINE_METRICS").as_deref() == Ok("1"))
}
pub(crate) fn receive(publications: usize, packets: usize) {
    PUBLICATIONS.fetch_add(publications as u64, Ordering::Relaxed);
    TURNS[packets.min(64)].fetch_add(1, Ordering::Relaxed);
}
/// Cumulative receive turns, before outbound routing and encryption (including tails).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Packet sizes at the outbound seam (all overlay transports).
    pub outbound: OutboundSnapshot,
    /// Wakes/polls of the bounded WG receiver.
    pub receiver: ReceiverSnapshot,
    /// Actual channel messages received, excluding a saved partial batch.
    pub publications: u64,
    /// Turns by packet count, exact buckets 0..64.
    pub batches: [u64; 65],
}
/// Read without an actor round trip.
pub fn snapshot() -> Snapshot {
    Snapshot {
        outbound: outbound_snapshot(),
        receiver: RECEIVER
            .get()
            .map_or_else(Default::default, |r| r.snapshot()),
        publications: PUBLICATIONS.load(Ordering::Relaxed),
        batches: core::array::from_fn(|i| TURNS[i].load(Ordering::Relaxed)),
    }
}

/// Receiver-side wake callbacks and polls; wake callbacks can coalesce into one scheduler turn.
#[derive(Default)]
pub struct ReceiverCounts {
    wakes: AtomicU64,
    polls: AtomicU64,
    pending: AtomicU64,
}
/// Cumulative observations on one channel receiver.
#[derive(Debug, Clone, Default)]
pub struct ReceiverSnapshot {
    /// Waker callbacks, rather than OS thread unparks.
    pub wakes: u64,
    /// Receiver poll calls, including cooperative-budget Pending results.
    pub polls: u64,
    /// Poll calls returning Pending.
    pub pending: u64,
}
impl ReceiverCounts {
    /// Read without locking the channel or messaging its owner.
    pub fn snapshot(&self) -> ReceiverSnapshot {
        ReceiverSnapshot {
            wakes: self.wakes.load(Ordering::Relaxed),
            polls: self.polls.load(Ordering::Relaxed),
            pending: self.pending.load(Ordering::Relaxed),
        }
    }
}
struct ReceiverWake {
    counts: std::sync::Arc<ReceiverCounts>,
    inner: std::task::Waker,
}
impl std::task::Wake for ReceiverWake {
    fn wake(self: std::sync::Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        self.counts.wakes.fetch_add(1, Ordering::Relaxed);
        self.inner.wake_by_ref();
    }
}
/// Metrics-only adapter. Keeps one forwarding Waker per receiver task, never one per packet.
pub struct ReceiverProbe {
    counts: std::sync::Arc<ReceiverCounts>,
    cached: Option<(std::task::Waker, std::task::Waker)>,
}
impl ReceiverProbe {
    /// Wrap a receiver's actual Waker callbacks with the supplied counter owner.
    pub fn new(counts: std::sync::Arc<ReceiverCounts>) -> Self {
        Self {
            counts,
            cached: None,
        }
    }
    /// Poll with the counted Waker; preserve Tokio's original readiness and cooperative budget.
    pub fn poll_recv<T>(
        &mut self,
        rx: &mut tokio::sync::mpsc::Receiver<T>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<T>> {
        if self
            .cached
            .as_ref()
            .is_none_or(|(original, _)| !original.will_wake(cx.waker()))
        {
            self.cached = Some((
                cx.waker().clone(),
                std::task::Waker::from(std::sync::Arc::new(ReceiverWake {
                    counts: self.counts.clone(),
                    inner: cx.waker().clone(),
                })),
            ));
        }
        self.counts.polls.fetch_add(1, Ordering::Relaxed);
        let mut counted = std::task::Context::from_waker(&self.cached.as_ref().unwrap().1);
        let result = rx.poll_recv(&mut counted);
        if result.is_pending() {
            self.counts.pending.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}
static RECEIVER: OnceLock<std::sync::Arc<ReceiverCounts>> = OnceLock::new();
pub(crate) fn receiver_probe() -> Option<ReceiverProbe> {
    enabled().then(|| {
        ReceiverProbe::new(
            RECEIVER
                .get_or_init(|| std::sync::Arc::new(Default::default()))
                .clone(),
        )
    })
}

static OUTBOUND_PACKETS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_BYTES: AtomicU64 = AtomicU64::new(0);
static TCP_DATA_BYTES: AtomicU64 = AtomicU64::new(0);
static TCP_DATA_SIZES: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
/// Inclusive upper bounds for IPv4 TCP payload bytes per offered data packet.
pub const TCP_DATA_SIZE_BOUNDS: [usize; 8] = [64, 256, 512, 1024, 1220, 1460, 4096, usize::MAX];
/// Packet shape offered to outbound routing/encryption, including the legacy N=1 path.
#[derive(Clone, Debug)]
pub struct OutboundSnapshot {
    /// All offered IP packets, before routing/ownership checks.
    pub packets: u64,
    /// Bytes in their buffers, before encryption/padding.
    pub bytes: u64,
    /// IPv4 TCP payload bytes, excluding empty ACK/SYN/FIN packets.
    pub tcp_data_bytes: u64,
    /// Data packet counts in TCP_DATA_SIZE_BOUNDS (retransmits count again).
    pub tcp_data_sizes: [u64; 8],
}
fn outbound_snapshot() -> OutboundSnapshot {
    OutboundSnapshot {
        packets: OUTBOUND_PACKETS.load(Ordering::Relaxed),
        bytes: OUTBOUND_BYTES.load(Ordering::Relaxed),
        tcp_data_bytes: TCP_DATA_BYTES.load(Ordering::Relaxed),
        tcp_data_sizes: core::array::from_fn(|i| TCP_DATA_SIZES[i].load(Ordering::Relaxed)),
    }
}
pub(crate) fn outbound(packets: &[ts_packet::PacketMut]) {
    let mut bytes = 0;
    let mut data_bytes = 0;
    let mut sizes = [0; 8];
    for packet in packets {
        let b = packet.as_ref();
        bytes += b.len() as u64;
        if b.len() < 40 || b[0] >> 4 != 4 || b[9] != 6 || b[6] & 0x3f != 0 || b[7] != 0 {
            continue;
        }
        let ip = usize::from(b[0] & 15) * 4;
        let total = usize::from(u16::from_be_bytes([b[2], b[3]]));
        if ip < 20 || total > b.len() || total < ip + 20 {
            continue;
        }
        let tcp = usize::from(b[ip + 12] >> 4) * 4;
        if tcp < 20 || total <= ip + tcp {
            continue;
        }
        let payload = total - ip - tcp;
        data_bytes += payload as u64;
        sizes[TCP_DATA_SIZE_BOUNDS
            .iter()
            .position(|bound| payload <= *bound)
            .unwrap()] += 1;
    }
    OUTBOUND_PACKETS.fetch_add(packets.len() as u64, Ordering::Relaxed);
    OUTBOUND_BYTES.fetch_add(bytes, Ordering::Relaxed);
    TCP_DATA_BYTES.fetch_add(data_bytes, Ordering::Relaxed);
    for (counter, count) in TCP_DATA_SIZES.iter().zip(sizes) {
        if count != 0 {
            counter.fetch_add(count, Ordering::Relaxed);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counted_receiver_preserves_readiness_and_observes_channel_wake() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let counts = std::sync::Arc::new(ReceiverCounts::default());
        let mut probe = ReceiverProbe::new(counts.clone());
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(probe.poll_recv(&mut rx, &mut cx).is_pending());
        tx.try_send(7).unwrap();
        assert_eq!(counts.snapshot().wakes, 1);
        assert_eq!(
            probe.poll_recv(&mut rx, &mut cx),
            std::task::Poll::Ready(Some(7))
        );
        assert_eq!(counts.snapshot().polls, 2);
        assert_eq!(counts.snapshot().pending, 1);
    }
}
