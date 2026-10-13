use alloc::{sync::Arc, vec::Vec};
use core::{
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures_util::task::AtomicWaker;
use netcore::{
    flume, smoltcp,
    smoltcp::{
        phy::{ChecksumCapabilities, DeviceCapabilities, Medium},
        time::Instant,
    },
};

/// Bidirectional pipe carrying byte buffer payloads.
///
/// This is like [`netcore::Pipe`], except that it also implements
/// [`AsyncWakeDevice`][netcore::AsyncWakeDevice], which needs a bit of fiddling to adapt.
pub struct WakingPipe {
    /// The send side of the pipe.
    pub rx: WakingPipeReceiver,
    /// The transmit side of the pipe.
    pub tx: WakingPipeSender,
}

/// A [`flume::Receiver`] wrapped to support [`AsyncWakeDevice`][netcore::AsyncWakeDevice].
///
/// It wakes the remote [`WakingPipeSender`] when a message is received.
pub struct WakingPipeReceiver {
    rx: flume::Receiver<PipeMessage>,
    /// [`flume::Receiver`] doesn't expose a way to poll until a value is ready without
    /// consuming it. This holds the consumed value.
    buffered_rx: alloc::vec::IntoIter<Bytes>,

    /// The waker that this end of the pipe polls on in `poll_rx`.
    ///
    /// It is woken by the remote (tx) end of the pipe when a packet is sent, i.e. the
    /// readiness state of `poll_rx` changes.
    self_waker: Arc<AtomicWaker>,

    /// The waker for the remote (tx) end of the pipe.
    ///
    /// We wake this when we receive a packet (i.e. make room in the pipe). That only
    /// matters if `rx` is a bounded channel.
    remote_waker: Arc<AtomicWaker>,

    /// The byte budget of this direction of the pipe, if it has one. Every packet taken out of
    /// `rx` gives its length back to it.
    budget: Option<Arc<ByteBudget>>,
}

/// A [`flume::Sender`] that wakes a remote [`WakingPipeReceiver`] when a message is sent.
pub struct WakingPipeSender {
    tx: flume::Sender<PipeMessage>,
    // Channel capacity is immutable; avoid flume's capacity mutex on unbounded transmit polls.
    bounded: bool,
    pub(crate) pending_egress: Option<Vec<Bytes>>,

    /// The waker this end of the pipe polls on in `poll_tx`.
    ///
    /// It is woken by the remote (rx) end of the pipe when a packet is received, i.e. the
    /// readiness state of `poll_tx` changes.
    ///
    /// This only matters if `self.tx` is a bounded channel, otherwise in the unbounded case
    /// we're always ready to send.
    self_waker: Arc<AtomicWaker>,

    /// The waker for the remote (rx) end of the pipe.
    ///
    /// We wake this when we send a packet.
    remote_waker: Arc<AtomicWaker>,

    /// The byte budget of this direction of the pipe, if it has one. Every packet put into `tx`
    /// charges its length to it.
    budget: Option<Arc<ByteBudget>>,
}

/// A security-barrier handle that discards packets still queued for a device.
/// Use only on the device's runtime, with admission paused. The ingress sender must use
/// individual packets (including deferred wakes), so no batch is buffered in the receiver.
#[derive(Clone)]
pub struct IngressDiscard {
    rx: flume::Receiver<PipeMessage>,
    budget: Option<Arc<ByteBudget>>,
    waker: Arc<AtomicWaker>,
}
impl IngressDiscard {
    /// Drop all pending ingress and release its byte budget; returns packet count.
    pub fn discard(&self) -> usize {
        let mut count = 0;
        while let Ok(message) = self.rx.try_recv() {
            match message {
                PipeMessage::One(b) => {
                    release(&self.budget, b.len());
                    count += 1;
                }
                PipeMessage::Batch(batch) => {
                    for b in batch {
                        release(&self.budget, b.len());
                        count += 1;
                    }
                }
            }
        }
        if count > 0 {
            self.waker.wake();
        }
        count
    }
}

/// Live bounded device-output occupancy, including packets buffered before publication.
#[derive(Clone)]
pub struct EgressQueue(Arc<ByteBudget>);
impl EgressQueue {
    /// Current packets and bytes; sampled gauges are not an atomic pair.
    pub fn occupancy(&self) -> (usize, usize) {
        (
            self.0.packets.load(Ordering::Relaxed),
            self.0.queued.load(Ordering::Relaxed),
        )
    }
}

enum PipeMessage {
    One(Bytes),
    Batch(Vec<Bytes>),
}

impl Clone for WakingPipeSender {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            bounded: self.bounded,
            pending_egress: None,
            self_waker: self.self_waker.clone(),
            remote_waker: self.remote_waker.clone(),
            budget: self.budget.clone(),
        }
    }
}

/// A cap on the bytes queued in one direction of a [`WakingPipe`], shared by the sender and the
/// receiver of that direction.
///
/// A packet-count bound alone does not cap memory: a packet's buffer is as long as the packet, and
/// what arrives from a peer is not limited to the stack's MTU (a DERP frame carries up to 64 KiB).
#[derive(Debug)]
struct ByteBudget {
    queued: AtomicUsize,
    max: usize,
    packets: AtomicUsize,
    max_packets: usize,
    max_packet_bytes: usize,
}

impl ByteBudget {
    /// Charge `len` bytes if that keeps the queue within `max`; report whether it did.
    fn try_charge(&self, len: usize) -> bool {
        self.queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                queued.checked_add(len).filter(|&n| n <= self.max)
            })
            .is_ok()
    }

    /// Charge `len` bytes whether or not that exceeds `max`, for the blocking senders, which are
    /// already bounded by the channel's packet count.
    fn charge(&self, len: usize) {
        self.queued.fetch_add(len, Ordering::AcqRel);
        if self.max_packet_bytes > 0 {
            self.packets.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Give back `len` bytes previously charged.
    fn release(&self, len: usize) {
        self.queued.fetch_sub(len, Ordering::AcqRel);
        if self.max_packet_bytes > 0 {
            self.packets.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn charge(budget: &Option<Arc<ByteBudget>>, len: usize) {
    if let Some(budget) = budget {
        budget.charge(len);
    }
}

fn release(budget: &Option<Arc<ByteBudget>>, len: usize) {
    if let Some(budget) = budget {
        budget.release(len);
    }
}

impl WakingPipe {
    pub(crate) fn bound_output(
        &mut self,
        consumer: &mut Self,
        max_packets: usize,
        mtu: usize,
    ) -> EgressQueue {
        let budget = Arc::new(ByteBudget {
            queued: AtomicUsize::new(0),
            max: max_packets.saturating_mul(mtu),
            packets: AtomicUsize::new(0),
            max_packets,
            max_packet_bytes: mtu,
        });
        self.tx.budget = Some(budget.clone());
        consumer.rx.budget = Some(budget.clone());
        EgressQueue(budget)
    }

    /// Construct a new pipe with the given optional capacity `limit`.
    pub fn new(limit: Option<usize>) -> (Self, Self) {
        if let Some(limit) = limit {
            Self::bounded(limit)
        } else {
            Self::unbounded()
        }
    }

    /// Construct a new unbounded pipe.
    pub fn unbounded() -> (Self, Self) {
        Self::channels(None, None)
    }

    /// Construct a new pipe that can carry at most `limit` packets.
    pub fn bounded(limit: usize) -> (Self, Self) {
        Self::channels(Some(limit), Some(limit))
    }

    /// Construct a pipe for a netstack device whose *ingress* is bounded and whose *egress* is not.
    ///
    /// The first end is the device's, the second the caller's. The caller-to-device direction holds
    /// at most `max_packets` packets and `max_bytes` bytes, and
    /// [`try_send`](WakingPipeSender::try_send) refuses a packet that would exceed either — the
    /// shape of a NIC rx ring, which drops when full. The device-to-caller direction stays
    /// unbounded: smoltcp emits through [`TxToken::consume`](smoltcp::phy::TxToken::consume),
    /// which is synchronous and cannot wait, so a full egress queue would stall the stack's poll
    /// loop rather than shed load.
    pub fn ingress_bounded(max_packets: usize, max_bytes: usize) -> (Self, Self) {
        let budget = Arc::new(ByteBudget {
            queued: AtomicUsize::new(0),
            max: max_bytes,
            packets: AtomicUsize::new(0),
            max_packets: usize::MAX,
            max_packet_bytes: 0,
        });
        let (mut device, mut caller) = Self::channels(Some(max_packets), None);

        device.rx.budget = Some(budget.clone());
        caller.tx.budget = Some(budget);

        (device, caller)
    }

    fn channels(ingress: Option<usize>, egress: Option<usize>) -> (Self, Self) {
        let channel = |limit| match limit {
            Some(n) => flume::bounded(n),
            None => flume::unbounded(),
        };
        let (tx2, rx1) = channel(ingress);
        let (tx1, rx2) = channel(egress);
        let pipe1_rx_waker = Arc::new(AtomicWaker::new());
        let pipe2_rx_waker = Arc::new(AtomicWaker::new());

        let pipe1_tx_waker = Arc::new(AtomicWaker::new());
        let pipe2_tx_waker = Arc::new(AtomicWaker::new());

        (
            Self {
                rx: WakingPipeReceiver {
                    rx: rx1,
                    buffered_rx: Vec::new().into_iter(),
                    self_waker: pipe1_rx_waker.clone(),
                    remote_waker: pipe2_tx_waker.clone(),
                    budget: None,
                },
                tx: WakingPipeSender {
                    tx: tx1,
                    bounded: egress.is_some(),
                    pending_egress: None,
                    remote_waker: pipe2_rx_waker.clone(),
                    self_waker: pipe1_tx_waker.clone(),
                    budget: None,
                },
            },
            Self {
                rx: WakingPipeReceiver {
                    rx: rx2,
                    buffered_rx: Vec::new().into_iter(),
                    self_waker: pipe2_rx_waker,
                    remote_waker: pipe1_tx_waker,
                    budget: None,
                },
                tx: WakingPipeSender {
                    tx: tx2,
                    bounded: ingress.is_some(),
                    pending_egress: None,
                    remote_waker: pipe1_rx_waker,
                    self_waker: pipe2_tx_waker,
                    budget: None,
                },
            },
        )
    }
}

impl WakingPipeReceiver {
    pub(crate) fn discard_handle(&self) -> IngressDiscard {
        IngressDiscard {
            rx: self.rx.clone(),
            budget: self.budget.clone(),
            waker: self.remote_waker.clone(),
        }
    }

    /// Receive a packet.
    pub fn recv(&mut self) -> Option<Bytes> {
        if let Some(buf) = self.buffered_rx.next() {
            return Some(buf);
        }

        let ret = self.rx.recv().ok();
        self.unpack(ret)
    }

    /// Receive a packet asynchronously.
    pub async fn recv_async(&mut self) -> Option<Bytes> {
        if let Some(buf) = self.buffered_rx.next() {
            return Some(buf);
        }

        let ret = self.rx.recv_async().await.ok();
        self.unpack(ret)
    }

    /// Receive a packet if it's possible to do so without blocking.
    pub fn try_recv(&mut self) -> Option<Bytes> {
        if let Some(buf) = self.buffered_rx.next() {
            return Some(buf);
        }

        let ret = self.rx.try_recv().ok();
        self.unpack(ret)
    }

    fn unpack(&mut self, message: Option<PipeMessage>) -> Option<Bytes> {
        match message? {
            PipeMessage::One(buf) => {
                self.taken(Some(&buf));
                Some(buf)
            }
            PipeMessage::Batch(bufs) => {
                for buf in &bufs {
                    self.taken(Some(buf));
                }
                self.buffered_rx = bufs.into_iter();
                self.buffered_rx.next()
            }
        }
    }

    /// Report whether there is a packet ready to be received.
    pub fn rx_ready(&self) -> bool {
        self.buffered_rx.len() != 0 || !self.rx.is_empty()
    }

    /// Account for a packet just taken out of the channel: give its bytes back to the budget and
    /// wake the sender, which may now have room.
    fn taken(&self, buf: Option<&Bytes>) {
        if let Some(buf) = buf {
            release(&self.budget, buf.len());
        }
        self.remote_waker.wake();
    }
}

impl WakingPipeSender {
    /// Send a packet, blocking until complete.
    pub fn send(&self, buf: &[u8]) {
        charge(&self.budget, buf.len());
        if let Err(_e) = self.tx.send(PipeMessage::One(Bytes::copy_from_slice(buf))) {
            release(&self.budget, buf.len());
            tracing::warn!("send dropped: remote end of pipe is gone");
            return;
        }

        self.remote_waker.wake();
    }

    /// Send a packet asynchronously.
    pub async fn send_async(&self, buf: &[u8]) {
        charge(&self.budget, buf.len());
        if let Err(_e) = self
            .tx
            .send_async(PipeMessage::One(Bytes::copy_from_slice(buf)))
            .await
        {
            release(&self.budget, buf.len());
            tracing::warn!("send dropped: remote end of pipe is gone");
            return;
        }

        self.remote_waker.wake();
    }

    /// Send a packet if it's possible to do so without blocking.
    ///
    /// Returns whether the packet was actually sent. On a pipe built by
    /// [`WakingPipe::ingress_bounded`], a packet that would take the queue past its byte budget is
    /// refused the same way as one that finds the queue full.
    pub fn try_send(&self, buf: &[u8]) -> bool {
        self.try_send_inner(buf, true)
    }

    /// Admit a packet without waking the device. Call `notify_receive` before yielding,
    /// returning or waiting, including when some of the batch's packets were refused.
    pub fn try_send_deferred(&self, buf: &[u8]) -> bool {
        self.try_send_inner(buf, false)
    }

    /// Publish receive readiness after synchronous batch admission.
    pub fn notify_receive(&self) {
        self.remote_waker.wake();
    }

    fn try_send_inner(&self, buf: &[u8], wake: bool) -> bool {
        if let Some(budget) = &self.budget
            && !budget.try_charge(buf.len())
        {
            return false;
        }

        match self
            .tx
            .try_send(PipeMessage::One(Bytes::copy_from_slice(buf)))
        {
            Ok(()) => {
                if wake {
                    self.remote_waker.wake();
                }
                true
            }
            Err(flume::TrySendError::Full(..)) => {
                release(&self.budget, buf.len());
                false
            }
            Err(flume::TrySendError::Disconnected(..)) => {
                release(&self.budget, buf.len());
                tracing::warn!("send dropped: remote end of pipe is gone");

                // Semantically, that the remote end was dropped can be thought of as deciding to
                // ignore all of our messages
                true
            }
        }
    }

    pub(crate) fn flush_egress(&mut self) {
        if let Some(pending) = &mut self.pending_egress {
            if pending.is_empty() {
                return;
            }
            let bufs = core::mem::take(pending);
            if self.tx.send(PipeMessage::Batch(bufs)).is_err() {
                tracing::warn!("remote end of pipe dropped on batch send");
            }
            self.remote_waker.wake();
        }
    }

    /// Report whether we can currently transmit.
    pub fn tx_ready(&self) -> bool {
        if let Some(budget) = &self.budget
            && budget.max_packet_bytes > 0
            && (budget.packets.load(Ordering::Acquire) >= budget.max_packets
                || budget
                    .queued
                    .load(Ordering::Acquire)
                    .saturating_add(budget.max_packet_bytes)
                    > budget.max)
        {
            return false;
        }
        !self.bounded || !self.tx.is_full()
    }
}

impl netcore::AsyncWakeDevice for WakingPipeDev {
    fn finish_turn(self: Pin<&mut Self>) {
        self.get_mut().pipe.tx.flush_egress();
    }

    #[tracing::instrument(name = "WakingPipeDev::poll_tx", skip_all, level = "trace", ret)]
    fn poll_tx<'cx>(self: Pin<&mut Self>, cx: &mut Context<'cx>) -> Poll<()> {
        self.pipe.tx.self_waker.register(cx.waker());

        if self.pipe.tx.tx_ready() {
            return Poll::Ready(());
        }

        Poll::Pending
    }

    #[tracing::instrument(name = "WakingPipeDev::poll_rx", skip_all, level = "trace", ret)]
    fn poll_rx<'cx>(mut self: Pin<&mut Self>, cx: &mut Context<'cx>) -> Poll<()> {
        self.pipe.rx.self_waker.register(cx.waker());

        if self.pipe.rx.rx_ready() {
            // Check tx readiness so that we return Poll::Ready when Device::receive is actually
            // ready, which only occurs when both TxToken and RxToken can be constructed.
            core::task::ready!(self.as_mut().poll_tx(cx));

            return Poll::Ready(());
        }

        Poll::Pending
    }
}

impl smoltcp::phy::TxToken for &mut WakingPipeSender {
    #[tracing::instrument(
        name = "WakingPipeSender::consume",
        skip_all,
        fields(len),
        level = "trace"
    )]
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut b = BytesMut::zeroed(len);

        let ret = f(&mut b);
        charge(&self.budget, len);
        if let Some(pending) = &mut self.pending_egress {
            pending.push(b.freeze());
            // Bound publication size even without an async work budget.
            if pending.len() == 64 {
                self.flush_egress();
            }
            return ret;
        }
        if self.tx.send(PipeMessage::One(b.freeze())).is_err() {
            release(&self.budget, len);
            tracing::warn!("remote end of dropped on send");
        }

        self.remote_waker.wake();

        ret
    }
}

// Preserve the sender's existing by-value TxToken API for external pipe users.
impl smoltcp::phy::TxToken for WakingPipeSender {
    fn consume<R, F>(mut self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let result = smoltcp::phy::TxToken::consume(&mut self, len, f);
        self.flush_egress();
        result
    }
}

pub struct RxToken(Bytes);

impl smoltcp::phy::RxToken for RxToken {
    #[tracing::instrument(name = "WakingPipeRx::consume", skip_all, level = "trace")]
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0)
    }
}

/// Wrapper around [`WakingPipe`] to implement [`smoltcp::phy::Device`].
///
/// Like [`netcore::PipeDev`] except that it implements
/// [`AsyncWakeDevice`][netcore::AsyncWakeDevice].
pub struct WakingPipeDev {
    /// End of a pipe that will be directly connected to the netstack, receiving packets
    /// to be sent and supplying packets to be received.
    pub pipe: WakingPipe,

    /// The type of network frame the pipe will carry.
    ///
    /// For our purposes, this will typically be [`Medium::Ip`].
    pub medium: Medium,
    /// The maximum packet size to be transmitted through the pipe.
    ///
    /// The implementation does not check or limit the actual size of packets flowing
    /// through it, this field is just informational for
    /// [`smoltcp::phy::Device::capabilities`].
    pub mtu: usize,
}

impl smoltcp::phy::Device for WakingPipeDev {
    type RxToken<'a>
        = RxToken
    where
        Self: 'a;

    type TxToken<'a>
        = &'a mut WakingPipeSender
    where
        Self: 'a;

    #[tracing::instrument(skip(self), level = "trace")]
    fn receive(&mut self, timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let _ = timestamp;
        if self.pipe.tx.tx.is_disconnected() || !self.pipe.tx.tx_ready() {
            return None;
        }
        let b = self.pipe.rx.try_recv()?;
        Some((RxToken(b), &mut self.pipe.tx))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        if self.pipe.tx.tx.is_disconnected() || !self.pipe.tx.tx_ready() {
            return None;
        }
        Some(&mut self.pipe.tx)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();

        caps.max_transmission_unit = self.mtu;
        caps.medium = self.medium;
        caps.checksum = ChecksumCapabilities::ignored();

        caps
    }
}

#[cfg(test)]
mod tests {
    /// `try_recv` on an empty pipe returns `None` instead of blocking.
    #[test]
    fn try_recv_does_not_block_on_empty_pipe() {
        let (mut a, b) = WakingPipe::unbounded();
        assert!(a.rx.try_recv().is_none());
        b.tx.send(b"x");
        assert_eq!(a.rx.try_recv().as_deref(), Some(&b"x"[..]));
        assert!(a.rx.try_recv().is_none());
    }

    use netcore::{
        AsyncWakeDevice,
        smoltcp::phy::{Device, RxToken as _, TxToken as _},
    };

    use super::*;

    fn device(pipe: WakingPipe) -> WakingPipeDev {
        WakingPipeDev {
            pipe,
            medium: Medium::Ip,
            mtu: 1280,
        }
    }

    /// Hand the device one packet and report its length, as the netstack's poll loop would.
    fn device_takes(dev: &mut WakingPipeDev) -> Option<usize> {
        let (rx, _tx) = dev.receive(Instant::from_millis(0))?;
        Some(rx.consume(|buf| buf.len()))
    }

    /// The ingress direction refuses past its packet count, and taking a packet makes room again.
    #[test]
    fn security_discard_releases_count_and_byte_limits() {
        let (device, caller) = WakingPipe::ingress_bounded(2, 80);
        let discard = device.rx.discard_handle();
        assert!(caller.tx.try_send_deferred(&[0; 40]));
        assert!(caller.tx.try_send_deferred(&[0; 40]));
        assert!(!caller.tx.try_send(&[0]));
        assert_eq!(discard.discard(), 2);
        assert_eq!(discard.discard(), 0);
        assert!(caller.tx.try_send(&[0; 80]));
        assert_eq!(discard.discard(), 1);
    }

    #[test]
    fn bounded_device_output_backpressures_without_blocking_tokens_and_keeps_batch_limit() {
        use smoltcp::phy::{Device, TxToken};
        for batching in [false, true] {
            let (pipe, mut caller) = WakingPipe::unbounded();
            let mut dev = device(pipe);
            dev.mtu = 100;
            let queue = dev.pipe.bound_output(&mut caller, 2, 100);
            if batching {
                dev.pipe.tx.pending_egress = Some(Vec::new());
            }
            for _ in 0..2 {
                dev.transmit(Instant::ZERO)
                    .unwrap()
                    .consume(80, |b| b.fill(1));
            }
            assert_eq!(queue.occupancy(), (2, 160));
            assert!(dev.transmit(Instant::ZERO).is_none());
            dev.pipe.tx.flush_egress();
            assert!(caller.rx.try_recv().is_some());
            if batching {
                assert!(caller.rx.try_recv().is_some());
            }
            assert!(dev.transmit(Instant::ZERO).is_some());
            while caller.rx.try_recv().is_some() {}
            assert_eq!(queue.occupancy(), (0, 0));
        }
    }

    #[test]
    fn ingress_refuses_past_packet_count() {
        let (dev, caller) = WakingPipe::ingress_bounded(2, usize::MAX);
        let mut dev = device(dev);

        assert!(caller.tx.try_send(&[1]));
        assert!(caller.tx.try_send(&[2]));
        assert!(
            !caller.tx.try_send(&[3]),
            "a third packet overflows the ring"
        );

        assert!(!caller.tx.tx_ready());
        assert_eq!(device_takes(&mut dev), Some(1));
        assert!(caller.tx.tx_ready());
        assert!(caller.tx.try_send(&[4]), "taking a packet frees its slot");
    }

    /// The ingress direction refuses a packet that would exceed its byte budget, even with free
    /// slots, and the device taking a packet gives its bytes back.
    #[test]
    fn ingress_refuses_past_byte_budget() {
        let (dev, caller) = WakingPipe::ingress_bounded(100, 3000);
        let mut dev = device(dev);

        assert!(caller.tx.try_send(&[0; 1000]));
        assert!(caller.tx.try_send(&[0; 1000]));
        assert!(
            !caller.tx.try_send(&[0; 1001]),
            "would be 3001 bytes queued"
        );
        assert!(caller.tx.try_send(&[0; 1000]), "exactly at the budget fits");
        assert!(!caller.tx.try_send(&[0; 1]));

        assert_eq!(device_takes(&mut dev), Some(1000));
        assert!(
            caller.tx.try_send(&[0; 1000]),
            "taking a packet frees its bytes"
        );
        assert!(!caller.tx.try_send(&[0; 1]));
    }

    /// A refused packet charges nothing: refusals on a full ring must not leak budget, or the queue
    /// would eventually refuse everything.
    #[test]
    fn refused_packet_does_not_consume_budget() {
        let (dev, caller) = WakingPipe::ingress_bounded(1, 1000);
        let mut dev = device(dev);

        assert!(caller.tx.try_send(&[0; 10]));
        for _ in 0..1000 {
            assert!(!caller.tx.try_send(&[0; 10]), "the one slot is taken");
        }

        assert_eq!(device_takes(&mut dev), Some(10));
        assert!(
            caller.tx.try_send(&[0; 1000]),
            "the whole budget is free again"
        );
    }

    #[test]
    fn async_polls_flush_buffered_tokens_before_returning() {
        polls_flush_buffered_tokens(false);
    }

    #[test]
    fn receive_only_polls_flush_buffered_tokens_before_returning() {
        polls_flush_buffered_tokens(true);
    }

    fn polls_flush_buffered_tokens(receive_only: bool) {
        use core::future::Future;
        let (pipe, mut caller) = WakingPipe::unbounded();
        let mut dev = device(pipe);
        dev.pipe.tx.pending_egress = Some(Vec::new());
        let mut stack = netcore::Netstack::new(Default::default(), Instant::ZERO);
        if receive_only {
            stack.set_ingress_budget(Some(1));
        } else {
            stack.set_work_budget(Some(1));
        }
        // Simulate a partial output turn, then make the driver consume a malformed packet.
        dev.transmit(Instant::ZERO)
            .unwrap()
            .consume(1, |b| b[0] = 42);
        caller.tx.send(&[0; 20]);
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        {
            let mut poller = stack.wait_io_async(Instant::ZERO, &mut dev);
            assert!(Pin::new(&mut poller).poll(&mut cx).is_ready());
        }
        assert_eq!(caller.rx.try_recv().unwrap().as_ref(), &[42]);
        // A partial turn also publishes when the stack makes no progress and is cancelled.
        dev.transmit(Instant::ZERO)
            .unwrap()
            .consume(1, |b| b[0] = 43);
        let mut poller = stack.wait_io_async(Instant::ZERO, &mut dev);
        assert!(Pin::new(&mut poller).poll(&mut cx).is_pending());
        drop(poller);
        assert_eq!(caller.rx.try_recv().unwrap().as_ref(), &[43]);
        assert!(caller.rx.try_recv().is_none());
    }

    #[test]
    fn deferred_ingress_wakes_once_and_preserves_queue_limits() {
        struct Count(AtomicUsize);
        impl std::task::Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let (dev, caller) = WakingPipe::ingress_bounded(2, 3);
        let mut dev = device(dev);
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = std::task::Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(Pin::new(&mut dev).poll_rx(&mut cx).is_pending());
        assert!(caller.tx.try_send_deferred(&[1]));
        assert!(caller.tx.try_send_deferred(&[2, 3]));
        assert!(!caller.tx.try_send_deferred(&[4]));
        assert_eq!(count.0.load(Ordering::Relaxed), 0);
        caller.tx.notify_receive();
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert_eq!(dev.pipe.rx.try_recv().unwrap().as_ref(), &[1]);
        assert_eq!(dev.pipe.rx.try_recv().unwrap().as_ref(), &[2, 3]);
        assert!(caller.tx.try_send_deferred(&[0; 3]), "all budget returned");
        caller.tx.notify_receive();
    }

    #[test]
    fn egress_batches_are_published_at_turn_end_and_bounded() {
        let (dev, mut caller) = WakingPipe::unbounded();
        let mut dev = device(dev);
        dev.pipe.tx.pending_egress = Some(Vec::new());
        for i in 0..63u8 {
            dev.transmit(Instant::ZERO)
                .unwrap()
                .consume(1, |b| b[0] = i);
        }
        assert!(caller.rx.try_recv().is_none());
        Pin::new(&mut dev).finish_turn();
        assert_eq!(
            caller.rx.rx.len(),
            1,
            "one channel publication for the turn"
        );
        for i in 0..63u8 {
            assert_eq!(caller.rx.try_recv().unwrap()[0], i);
        }
        assert!(caller.rx.try_recv().is_none());
        for i in 0..65u8 {
            dev.transmit(Instant::ZERO)
                .unwrap()
                .consume(1, |b| b[0] = i);
        }
        assert_eq!(caller.rx.rx.len(), 1, "full chunks publish without waiting");
        Pin::new(&mut dev).finish_turn();
        assert_eq!(caller.rx.rx.len(), 2);
        for i in 0..65u8 {
            assert_eq!(caller.rx.try_recv().unwrap()[0], i);
        }
        Pin::new(&mut dev).finish_turn();
        assert!(
            caller.rx.try_recv().is_none(),
            "empty turns publish nothing"
        );
    }

    /// The egress direction stays unbounded: smoltcp's `TxToken::consume` cannot wait, so a bounded
    /// egress would block the stack's poll loop. Emitting far past the ingress limits must neither
    /// block nor drop.
    #[test]
    fn egress_is_not_bounded() {
        let (dev, mut caller) = WakingPipe::ingress_bounded(2, 100);
        let mut dev = device(dev);

        for i in 0..1000u32 {
            let tx = dev
                .transmit(Instant::from_millis(0))
                .expect("egress is always ready");
            tx.consume(1280, |buf| buf[..4].copy_from_slice(&i.to_be_bytes()));
        }

        assert!(dev.pipe.tx.tx_ready(), "unbounded egress is always ready");
        for i in 0..1000u32 {
            let buf = caller.rx.recv().expect("every emitted packet is delivered");
            assert_eq!(buf.len(), 1280);
            assert_eq!(buf[..4], i.to_be_bytes());
        }
    }
}
