//! Stage 1: authenticated flow dispatch; WG, app stack and underlay retain their owners.
use std::{
    collections::{HashMap, hash_map::RandomState},
    hash::BuildHasher,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use kameo::{
    actor::ActorRef,
    message::{Context, Message},
};
use netstack::netcore::Channel;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot},
    task::JoinSet,
};
use ts_bart::RoutingTable;
use ts_forwarder::{RouteTable, RouteUpdater, ShardAdmission, ShardResources};
use ts_packet::PacketMut;
use ts_transport::PeerId;

use crate::{
    Error,
    dataplane::{OverlayFromDataplane, OverlayToDataplane},
    env::Env,
    forwarder_actor::ForwarderActor,
    packetfilter::PacketFilterState,
    src_filter::SourceFilterState,
};
mod fragments;

const QUEUE_PACKETS: usize = 4096;
const QUEUE_BYTES: usize = 4096 * 1280;

pub(crate) fn count_from_env() -> std::io::Result<usize> {
    let value = std::env::var_os("TS_EXPERIMENT_FORWARDER_SHARDS")
        .map(|value| {
            value.into_string().map_err(|_| {
                std::io::Error::other(
                    "TS_EXPERIMENT_FORWARDER_SHARDS must be a UTF-8 integer in 1..=64",
                )
            })
        })
        .transpose()?;
    count(
        value.as_deref(),
        std::env::var("TS_EXPERIMENT_PIPELINE_THREAD").as_deref() == Ok("1"),
    )
}
fn count(value: Option<&str>, pipeline: bool) -> std::io::Result<usize> {
    let n = value.unwrap_or("1").parse::<usize>().map_err(|_| {
        std::io::Error::other("TS_EXPERIMENT_FORWARDER_SHARDS must be an integer in 1..=64")
    })?;
    if !(1..=64).contains(&n) {
        return Err(std::io::Error::other(
            "TS_EXPERIMENT_FORWARDER_SHARDS must be in 1..=64",
        ));
    }
    if n > 1 && pipeline {
        return Err(std::io::Error::other(
            "TS_EXPERIMENT_FORWARDER_SHARDS>1 is incompatible with TS_EXPERIMENT_PIPELINE_THREAD=1",
        ));
    }
    Ok(n)
}

/// Hash full canonical endpoints, protocol and authenticated WG peer. TCP flags are excluded.
#[derive(Clone)]
struct Selector {
    key: RandomState,
    shards: usize,
}
impl Selector {
    fn new(shards: usize) -> Self {
        Self {
            key: RandomState::new(),
            shards,
        }
    }
    fn hash(&self, peer: PeerId, bytes: &[u8]) -> Option<u64> {
        let ip = ipv4(bytes)?;
        let offset = usize::from(bytes[0] & 15) * 4;
        if bytes[6] & 0x1f != 0 || bytes[7] != 0 {
            return None;
        }
        let protocol = bytes[9];
        let ports = if matches!(protocol, 6 | 17) {
            let b = bytes.get(offset..offset + 4)?;
            (
                u16::from_be_bytes([b[0], b[1]]),
                u16::from_be_bytes([b[2], b[3]]),
            )
        } else {
            (0, 0)
        };
        let mut endpoints = [(ip.0, ports.0), (ip.1, ports.1)];
        endpoints.sort_unstable();
        Some(self.key.hash_one((peer.0, protocol, endpoints)))
    }
    fn owner(&self, peer: PeerId, bytes: &[u8]) -> Option<usize> {
        let offset = usize::from(*bytes.first()? & 15) * 4;
        // ICMP errors quote the original flow. Echo and other protocols use shard 0.
        if bytes.get(9) == Some(&1) {
            if matches!(bytes.get(offset), Some(3 | 4 | 5 | 11 | 12)) {
                return self
                    .hash(peer, bytes.get(offset + 8..)?)
                    .map(|h| h as usize % self.shards);
            }
            return Some(0);
        }
        self.hash(peer, bytes).map(|h| h as usize % self.shards)
    }
}
fn ipv4(b: &[u8]) -> Option<(IpAddr, IpAddr)> {
    if b.len() < 20 || b[0] >> 4 != 4 {
        return None;
    }
    let len = usize::from(b[0] & 15) * 4;
    if len < 20 || len > b.len() {
        return None;
    }
    Some((
        IpAddr::from(<[u8; 4]>::try_from(&b[12..16]).ok()?),
        IpAddr::from(<[u8; 4]>::try_from(&b[16..20]).ok()?),
    ))
}

#[derive(Default)]
pub(crate) struct Metrics {
    index: AtomicUsize,
    packets_in: AtomicU64,
    packets_out: AtomicU64,
    drops: AtomicU64,
    queued: AtomicUsize,
    queued_bytes: AtomicUsize,
    peak: AtomicUsize,
    pub(crate) flows: Arc<AtomicUsize>,
    gate: Mutex<Option<ts_forwarder::PortGate>>,
    socket_buffer_bytes: AtomicUsize,
    tcp_sockets: Mutex<Option<Arc<AtomicUsize>>>,
    egress_queue: Mutex<Option<netstack::EgressQueue>>,
    ingress_ring_bytes: AtomicUsize,
}
#[derive(Debug, Clone)]
pub struct ShardSnapshot {
    pub index: usize,
    pub gate: ts_forwarder::GateSnapshot,
    pub tcp_bytes_per_socket: usize,
    pub tcp_sockets: usize,
    pub egress_queued: usize,
    pub egress_queued_bytes: usize,
    pub tcp_buffer_bytes: usize,
    pub ingress_ring_limit_bytes: usize,
    pub packets_in: u64,
    pub packets_out: u64,
    pub flows: usize,
    pub queued: usize,
    pub queued_bytes: usize,
    pub peak: usize,
    pub drops: u64,
}
static METRICS: Mutex<Vec<std::sync::Weak<Metrics>>> = Mutex::new(Vec::new());
pub(crate) fn snapshots() -> Vec<ShardSnapshot> {
    METRICS
        .lock()
        .unwrap()
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .map(|m| {
            let tcp_sockets = m
                .tcp_sockets
                .lock()
                .unwrap()
                .as_ref()
                .map_or(0, |c| c.load(Ordering::Relaxed));
            let (egress_queued, egress_queued_bytes) = m
                .egress_queue
                .lock()
                .unwrap()
                .as_ref()
                .map_or((0, 0), netstack::EgressQueue::occupancy);
            ShardSnapshot {
                egress_queued,
                egress_queued_bytes,
                tcp_sockets,
                tcp_buffer_bytes: tcp_sockets
                    .saturating_mul(m.socket_buffer_bytes.load(Ordering::Relaxed)),
                index: m.index.load(Ordering::Relaxed),
                gate: m
                    .gate
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map_or_else(Default::default, ts_forwarder::PortGate::snapshot),
                tcp_bytes_per_socket: m.socket_buffer_bytes.load(Ordering::Relaxed),
                ingress_ring_limit_bytes: m.ingress_ring_bytes.load(Ordering::Relaxed),
                packets_in: m.packets_in.load(Ordering::Relaxed),
                packets_out: m.packets_out.load(Ordering::Relaxed),
                flows: m.flows.load(Ordering::Relaxed),
                queued: m.queued.load(Ordering::Relaxed),
                queued_bytes: m.queued_bytes.load(Ordering::Relaxed),
                peak: m.peak.load(Ordering::Relaxed),
                drops: m.drops.load(Ordering::Relaxed),
            }
        })
        .collect()
}
impl Metrics {
    pub(crate) fn set_egress_queue(&self, queue: Option<netstack::EgressQueue>) {
        *self.egress_queue.lock().unwrap() = queue;
    }
    pub(crate) fn set_socket_counter(&self, counter: Arc<AtomicUsize>) {
        *self.tcp_sockets.lock().unwrap() = Some(counter);
    }
    pub(crate) fn configure(&self, config: &netstack::netcore::Config) {
        self.socket_buffer_bytes
            .store(config.tcp_buffer_size * 2, Ordering::Relaxed);
        self.ingress_ring_bytes.store(
            config.mtu * crate::netstack_actor::INGRESS_QUEUE_PACKETS,
            Ordering::Relaxed,
        );
    }
    pub(crate) fn set_gate(&self, gate: Option<ts_forwarder::PortGate>) {
        *self.gate.lock().unwrap() = gate;
    }

    pub(crate) fn out_packets(&self, n: usize) {
        self.packets_out.fetch_add(n as u64, Ordering::Relaxed);
    }
    fn drop_packet(&self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
pub(crate) struct Queued {
    peer: PeerId,
    packet: Option<PacketMut>,
    length: usize,
    _bytes: OwnedSemaphorePermit,
    metrics: Option<Arc<Metrics>>,
}
impl Drop for Queued {
    fn drop(&mut self) {
        if let Some(m) = &self.metrics {
            m.queued.fetch_sub(1, Ordering::Relaxed);
            m.queued_bytes.fetch_sub(self.length, Ordering::Relaxed);
        }
    }
}
#[derive(Clone)]
struct Queue {
    tx: mpsc::Sender<Queued>,
    bytes: Arc<Semaphore>,
    metrics: Option<Arc<Metrics>>,
}
impl Queue {
    fn send(&self, peer: PeerId, packet: PacketMut) {
        let n = packet.as_ref().len();
        let bytes = u32::try_from(n)
            .ok()
            .and_then(|n| self.bytes.clone().try_acquire_many_owned(n).ok());
        let Some(bytes) = bytes else {
            if let Some(m) = &self.metrics {
                m.drop_packet();
            }
            return;
        };
        let Ok(slot) = self.tx.try_reserve() else {
            if let Some(m) = &self.metrics {
                m.drop_packet();
            }
            return;
        };
        if let Some(m) = &self.metrics {
            let queued = m.queued.fetch_add(1, Ordering::Relaxed) + 1;
            m.queued_bytes.fetch_add(n, Ordering::Relaxed);
            m.peak.fetch_max(queued, Ordering::Relaxed);
        }
        slot.send(Queued {
            peer,
            packet: Some(packet),
            length: n,
            _bytes: bytes,
            metrics: self.metrics.clone(),
        });
    }
}

pub(crate) enum Input {
    Legacy(OverlayFromDataplane),
    Shard {
        packets: mpsc::Receiver<Queued>,
        security: Arc<Mutex<Security>>,
        metrics: Option<Arc<Metrics>>,
        owners: Arc<Mutex<HashMap<IpAddr, PeerId>>>,
    },
}
impl Input {
    pub(crate) fn metrics(&self) -> Option<Arc<Metrics>> {
        match self {
            Self::Legacy(_) => None,
            Self::Shard { metrics, .. } => metrics.clone(),
        }
    }
}
pub(crate) struct Security {
    sources: SourceFilterState,
    filter: PacketFilterState,
}
pub(crate) async fn pump(
    mut packets: mpsc::Receiver<Queued>,
    security: Arc<Mutex<Security>>,
    metrics: Option<Arc<Metrics>>,
    owners: Arc<Mutex<HashMap<IpAddr, PeerId>>>,
    mut admit: impl FnMut(&[u8]) -> bool,
    mut flush: impl FnMut(),
) {
    let mut admission = ts_dataplane::InboundAdmission::default();
    let mut turns = 0;
    while let Some(mut queued) = packets.recv().await {
        let mut batch = vec![queued.packet.take().unwrap()];
        {
            let security_guard = security.lock().unwrap();
            {
                let security = &*security_guard;
                admission.retain(
                    queued.peer,
                    &mut batch,
                    &security.sources.0,
                    security.filter.0.as_ref(),
                );
            }
            if let Some(packet) = batch.pop() {
                if let Some(src) = packet.get_src_addr() {
                    // Bounded ownership ledger; a security barrier checks it before acknowledging.
                    let mut owners = owners.lock().unwrap();
                    if owners.len() >= 8192 && !owners.contains_key(&src) {
                        if let Some(m) = &metrics {
                            m.drop_packet();
                        }
                        continue;
                    }
                    if owners.get(&src).is_some_and(|peer| *peer != queued.peer) {
                        if let Some(m) = &metrics {
                            m.drop_packet();
                        }
                        continue;
                    }
                    owners.insert(src, queued.peer);
                }
                if let Some(m) = &metrics {
                    m.packets_in.fetch_add(1, Ordering::Relaxed);
                }
                if !admit(packet.as_ref())
                    && let Some(m) = &metrics
                {
                    m.drop_packet();
                }
            } else if let Some(m) = &metrics {
                m.drop_packet();
            }
        }
        drop(queued);
        turns += 1;
        if turns == 128 || packets.is_empty() {
            flush();
        }
        if turns == 128 {
            turns = 0;
            tokio::task::yield_now().await;
        }
    }
    flush();
}

struct Dispatcher {
    admission: Mutex<()>,
    failed: tokio::sync::watch::Receiver<bool>,
    selector: Selector,
    exit_dns: bool,
    queues: Vec<Queue>,
    fragments: Mutex<fragments::Reassembly>,
    routes: Mutex<RouteTable>,
    paused: AtomicBool,
    stopped: Arc<AtomicBool>,
    reject: Mutex<super::forwarder_actor::RejectBudget>,
    up: OverlayToDataplane,
}
impl Dispatcher {
    fn send(&self, peer: PeerId, packets: Vec<PacketMut>) {
        let _admission = self.admission.lock().unwrap();
        if *self.failed.borrow()
            || self.paused.load(Ordering::Acquire)
            || self.stopped.load(Ordering::Acquire)
        {
            return;
        }
        for packet in packets {
            let bytes = packet.as_ref();
            if bytes.first().is_some_and(|v| v >> 4 == 6) {
                if self.reject.lock().unwrap().take()
                    && let Some(reply) = ts_forwarder::reject_ipv6(bytes)
                {
                    drop(self.up.send(vec![reply.into()]));
                }
                continue;
            }
            let Some((_, dst)) = ipv4(bytes) else {
                continue;
            };
            let class = self.routes.lock().unwrap().classify(dst);
            if class.is_none() {
                continue;
            }
            let packet = if bytes[6] & 0x3f != 0 || bytes[7] != 0 {
                let Some(packet) = self.fragments.lock().unwrap().push(peer, packet) else {
                    continue;
                };
                packet
            } else {
                packet
            };
            if !destination_allowed(class.unwrap(), dst, packet.as_ref(), self.exit_dns) {
                continue;
            }
            if let Some(owner) = self.selector.owner(peer, packet.as_ref()) {
                self.queues[owner].send(peer, packet);
            }
        }
    }
}

fn destination_allowed(
    class: ts_forwarder::FlowClass,
    dst: IpAddr,
    bytes: &[u8],
    exit_dns: bool,
) -> bool {
    if class != ts_forwarder::FlowClass::ExitNode
        || !ts_forwarder::exit_dst_is_forbidden(SocketAddr::new(dst, 0))
    {
        return true;
    }
    let offset = usize::from(bytes[0] & 15) * 4;
    // Preserve the operator-configured DNS redirection, which dials the fixed resolver and
    // never the peer-selected forbidden destination. The ordinary guard still runs in dialers.
    exit_dns
        && matches!(bytes[9], 6 | 17)
        && bytes.get(offset + 2..offset + 4) == Some(&53u16.to_be_bytes())
}

pub(crate) struct Group {
    pub(crate) owners: ts_dataplane::ForwarderSourceOwners,
    pub(crate) sink: ts_dataplane::AuthenticatedOverlaySink,
    stops: Vec<crate::pipeline::PipelineStop>,
    runners: Vec<tokio::task::AbortHandle>,
    failed: tokio::sync::watch::Sender<bool>,
    dispatcher: Arc<Dispatcher>,
    #[cfg(test)]
    admission: ShardAdmission,
    controls: Vec<mpsc::Sender<Control>>,
    routes: Vec<RouteUpdater>,
    // Runtime owners never travel into tasks executing on those runtimes.
    #[cfg(test)]
    handles: Vec<tokio::runtime::Handle>,
    _threads: Vec<crate::pipeline::PipelineThread>,
}
impl Drop for Group {
    fn drop(&mut self) {
        self.dispatcher.stopped.store(true, Ordering::Release);
        self.failed.send_replace(true);
        for stop in &self.stops {
            stop.stop();
        }
        for runner in &self.runners {
            runner.abort();
        }
    }
}
enum Control {
    #[cfg(test)]
    Panic,
    #[cfg(test)]
    Probe(oneshot::Sender<(bool, usize)>),
    Sources(SourceFilterState, oneshot::Sender<()>),
    Filter(PacketFilterState, oneshot::Sender<()>),
}
fn gone() -> Error {
    Error {
        kind: crate::ErrorKind::ActorGone,
        message_ty: None,
        target_actor: None,
    }
}
impl Group {
    pub(crate) async fn start(
        env: Env,
        config: netstack::netcore::Config,
        up: OverlayToDataplane,
        actor: ActorRef<ForwarderActor>,
    ) -> Result<ForwarderActor, Error> {
        let n = env.forwarder_shards;
        let limits = ShardAdmission::new(n);
        let owners = Arc::new(Mutex::new(HashMap::new()));
        let mut stops = Vec::new();
        let mut runners = Vec::new();
        let mut threads = Vec::new();
        let mut handles = Vec::new();
        let mut queues = Vec::new();
        let mut routes = Vec::new();
        let mut controls = Vec::new();
        let mut joins = JoinSet::new();
        let mut channels: Vec<Channel> = Vec::new();
        let stopped = Arc::new(AtomicBool::new(false));
        let (failed, mut failure) = tokio::sync::watch::channel(false);
        let monitor_actor = actor.clone();
        let stop = stopped.clone();
        for i in 0..n {
            let handle = if let Some(handle) = env.forwarder_handles.get(i) {
                stops.push(env.forwarder_stops[i].clone());
                handle.clone()
            } else {
                let thread = crate::pipeline::PipelineThread::named(&format!("ts-fwd-shard-{i}"))
                    .map_err(|e| {
                    tracing::error!(error=%e, "starting forwarder shard");
                    gone()
                })?;
                let handle = thread.handle();
                stops.push(thread.stop_handle());
                threads.push(thread);
                handle
            };
            handles.push(handle.clone());
            let metrics = crate::pipeline_metrics::enabled().then(|| Arc::new(Metrics::default()));
            if let Some(m) = &metrics {
                m.index.store(i, Ordering::Relaxed);
                let mut registry = METRICS.lock().unwrap();
                registry.retain(|m| m.strong_count() > 0);
                registry.push(Arc::downgrade(m));
            }
            let (tx, packets) = mpsc::channel(QUEUE_PACKETS);
            queues.push(Queue {
                tx,
                bytes: Arc::new(Semaphore::new(QUEUE_BYTES)),
                metrics: metrics.clone(),
            });
            let (control_tx, mut control_rx) = mpsc::channel(8);
            controls.push(control_tx);
            let security = Arc::new(Mutex::new(Security {
                sources: SourceFilterState(Arc::new(Default::default())),
                filter: PacketFilterState(Arc::new(ts_packetfilter::DropAllFilter)),
            }));
            let owners = owners.clone();
            let input = Input::Shard {
                packets,
                security: security.clone(),
                metrics: metrics.clone(),
                owners: owners.clone(),
            };
            let mut shard_env = env.clone();
            shard_env.forwarder_shards = 1;
            shard_env.packet_runtime = None;
            let resources = ShardResources {
                admission: limits.clone(),
                failed: failed.clone(),
                flows: metrics.map(|m| m.flows.clone()),
            };
            let config = config.clone();
            let up = up.clone();
            let mut shard = handle
                .spawn(async move {
                    resources
                        .scope(ForwarderActor::single(shard_env, config, up, input))
                        .await
                })
                .await
                .map_err(|_| gone())??;
            channels.push(shard.channel.clone());
            routes.push(shard.route_updater.clone());
            let fail = failed.clone();
            let ingress_discard = shard.ingress_discard.clone().unwrap();
            let gate = shard.gate.clone();
            shard._joinset.spawn_on(
                async move {
                    while let Some(control) = control_rx.recv().await {
                        if let Some(gate) = &gate {
                            gate.discard_held();
                        }
                        ingress_discard.discard();
                        match control {
                            #[cfg(test)]
                            Control::Panic => panic!("injected shard panic"),
                            #[cfg(test)]
                            Control::Probe(tx) => {
                                let state = security.lock().unwrap();
                                let mut p = vec![tests::packet(6, 1, 443).into()];
                                ts_dataplane::InboundAdmission::default().retain(
                                    PeerId(1),
                                    &mut p,
                                    &state.sources.0,
                                    state.filter.0.as_ref(),
                                );
                                let _ = tx.send((!p.is_empty(), owners.lock().unwrap().len()));
                            }
                            Control::Sources(state, ack) => {
                                // Reassigned/removed sources cannot inherit sockets from an old peer.
                                let mut security = security.lock().unwrap();
                                if owners
                                    .lock()
                                    .unwrap()
                                    .iter()
                                    .any(|(ip, peer)| state.0.lookup(*ip) != Some(peer))
                                {
                                    tracing::error!(
                                        "forwarder source ownership changed; stopping all shards"
                                    );
                                    fail.send_replace(true);
                                    return;
                                }
                                security.sources = state;
                                let _ = ack.send(());
                            }
                            Control::Filter(state, ack) => {
                                security.lock().unwrap().filter = state;
                                let _ = ack.send(());
                            }
                        }
                    }
                },
                &handle,
            );
            let fail = failed.clone();
            let run = handle.spawn(async move {
                let ended = shard._joinset.join_next().await;
                tracing::error!(
                    ?ended,
                    shard = i,
                    "forwarder shard task exited; stopping forwarding"
                );
                fail.send_replace(true);
            });
            runners.push(run.abort_handle());
            let fail = failed.clone();
            joins.spawn(async move {
                drop(run.await);
                fail.send_replace(true);
            });
        }
        let monitor_stops = stops.clone();
        crate::pipeline::spawn_on(&mut joins, env.supervisor_runtime.as_ref(), async move {
            drop(failure.wait_for(|v| *v).await);
            stop.store(true, Ordering::Release);
            for stop in monitor_stops {
                stop.stop();
            }
            monitor_actor.kill();
        });
        let dispatcher = Arc::new(Dispatcher {
            admission: Mutex::new(()),
            failed: failed.subscribe(),
            selector: Selector::new(n),
            exit_dns: env.forward_exit_egress
                && env.exit_proxy.is_none()
                && env.exit_node_dns.is_some(),
            queues,
            fragments: Mutex::new(Default::default()),
            routes: Mutex::new(RouteTable::new(env.forward_routes.iter().copied())),
            paused: AtomicBool::new(false),
            stopped,
            reject: Mutex::new(super::forwarder_actor::RejectBudget::new()),
            up,
        });
        let sink_dispatcher = dispatcher.clone();
        let group = Self {
            owners,
            sink: Arc::new(move |peer, packets| sink_dispatcher.send(peer, packets)),
            stops,
            runners,
            failed,
            dispatcher,
            #[cfg(test)]
            admission: limits,
            controls,
            routes,
            #[cfg(test)]
            handles,
            _threads: threads,
        };
        // DataplaneActor awaits this owner from its existing guaranteed, ordered bus handlers
        // before committing security state; no second bus fan-out or coalesced security watch.
        Ok(ForwarderActor {
            _joinset: joins,
            channel: channels.remove(0),
            route_updater: group.routes[0].clone(),
            ingress_discard: None,
            gate: None,
            shards: Some(group),
        })
    }
    pub(crate) fn update_routes(&self, routes: RouteTable) {
        *self.dispatcher.routes.lock().unwrap() = routes.clone();
        for update in &self.routes {
            update.update(routes.clone());
        }
    }
    async fn security(
        &self,
        sources: Option<SourceFilterState>,
        filter: Option<PacketFilterState>,
    ) -> Result<(), Error> {
        {
            // Wait for an already-entered authenticated batch before clearing fragment state.
            // Holding this guard across the pause closes the check-then-admit race; release it
            // before awaiting any shard so the WG owner never waits on a shard acknowledgement.
            let _admission = self.dispatcher.admission.lock().unwrap();
            self.dispatcher.paused.store(true, Ordering::Release);
            self.dispatcher.fragments.lock().unwrap().clear();
        }
        // Await every runtime's acknowledgement. A full control queue cannot lose revocations.
        for tx in &self.controls {
            let (ack, rx) = oneshot::channel();
            let message = if let Some(state) = &sources {
                Control::Sources(state.clone(), ack)
            } else {
                Control::Filter(filter.as_ref().unwrap().clone(), ack)
            };
            tx.send(message).await.map_err(|_| gone())?;
            rx.await.map_err(|_| gone())?;
        }
        self.dispatcher.paused.store(false, Ordering::Release);
        Ok(())
    }
}
impl Message<SourceFilterState> for ForwarderActor {
    type Reply = ();
    async fn handle(&mut self, state: SourceFilterState, ctx: &mut Context<Self, ()>) {
        if let Some(group) = &self.shards
            && group.security(Some(state), None).await.is_err()
        {
            ctx.actor_ref().kill();
        }
    }
}
impl Message<PacketFilterState> for ForwarderActor {
    type Reply = ();
    async fn handle(&mut self, state: PacketFilterState, ctx: &mut Context<Self, ()>) {
        if let Some(group) = &self.shards
            && group.security(None, Some(state)).await.is_err()
        {
            ctx.actor_ref().kill();
        }
    }
}

#[cfg(test)]
mod tests;
