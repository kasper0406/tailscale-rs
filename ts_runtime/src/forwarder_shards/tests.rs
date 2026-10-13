use std::time::Duration;

use kameo::actor::Spawn;
use netstack::{CreateSocket, HasChannel, netcore::NetstackControl};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
mod splice_probe;
pub(super) fn packet(protocol: u8, src_port: u16, dst_port: u16) -> Vec<u8> {
    let mut b = vec![0; 40];
    b[0] = 0x45;
    b[2..4].copy_from_slice(&40u16.to_be_bytes());
    b[8] = 64;
    b[9] = protocol;
    b[12..16].copy_from_slice(&[100, 64, 0, 1]);
    b[16..20].copy_from_slice(&[127, 0, 0, 1]);
    b[20..22].copy_from_slice(&src_port.to_be_bytes());
    b[22..24].copy_from_slice(&dst_port.to_be_bytes());
    b[32] = 0x50;
    b[33] = 2;
    if protocol == 17 {
        b[24..26].copy_from_slice(&20u16.to_be_bytes());
    }
    b
}
#[test]
fn selection_is_bidirectional_sticky_peer_namespaced_and_keyed() {
    let a = Selector::new(8);
    let b = Selector::new(8);
    let syn = packet(6, 41000, 443);
    let mut reversed = syn.clone();
    reversed[12..16].copy_from_slice(&syn[16..20]);
    reversed[16..20].copy_from_slice(&syn[12..16]);
    reversed[20..22].copy_from_slice(&syn[22..24]);
    reversed[22..24].copy_from_slice(&syn[20..22]);
    let mut later = syn.clone();
    later[33] = 0x11;
    assert_eq!(a.hash(PeerId(7), &syn), a.hash(PeerId(7), &reversed));
    assert_eq!(a.owner(PeerId(7), &syn), a.owner(PeerId(7), &syn.clone()));
    assert_eq!(a.owner(PeerId(7), &syn), a.owner(PeerId(7), &later));
    assert_ne!(a.hash(PeerId(7), &syn), a.hash(PeerId(8), &syn));
    assert_ne!(a.hash(PeerId(7), &syn), b.hash(PeerId(7), &syn));
    let one = Selector::new(1);
    assert_eq!(one.owner(PeerId(7), &syn), Some(0));
    let mut seen = [false; 8];
    for port in 10000..11000 {
        seen[a.owner(PeerId(7), &packet(6, port, 443)).unwrap()] = true;
    }
    assert!(seen.into_iter().all(|x| x));
}
#[test]
fn icmp_errors_follow_quoted_flow_and_echo_has_service_owner() {
    let selector = Selector::new(4);
    let flow = packet(17, 12345, 53);
    let mut error = packet(1, 0, 0);
    error.truncate(28);
    error[20] = 3;
    error.extend_from_slice(&flow[..28]);
    assert_eq!(
        selector.owner(PeerId(1), &error),
        selector.owner(PeerId(1), &flow)
    );
    error[20] = 8;
    assert_eq!(selector.owner(PeerId(1), &error), Some(0));
    error[20] = 3;
    error.truncate(30);
    assert_eq!(selector.owner(PeerId(1), &error), None);
}
#[test]
fn n1_uses_existing_path_and_conflicting_modes_refuse_startup() {
    assert_eq!(count(None, false).unwrap(), 1);
    assert_eq!(count(Some("1"), false).unwrap(), 1);
    assert_eq!(count(Some("1"), true).unwrap(), 1);
    assert!(
        count(Some("4"), true)
            .unwrap_err()
            .to_string()
            .contains("incompatible")
    );
    for invalid in ["0", "65", "-1", "4.0", ""] {
        assert!(count(Some(invalid), false).is_err());
    }
}
#[tokio::test]
async fn queue_has_both_packet_and_byte_bounds_and_releases_on_drop() {
    let (tx, mut rx) = mpsc::channel(2);
    let metrics = Arc::new(Metrics::default());
    let queue = Queue {
        tx,
        packets: PacketBudget::new(4096),
        bytes: Arc::new(Semaphore::new(80)),
        metrics: Some(metrics.clone()),
    };
    queue.send(PeerId(1), packet(6, 1, 2).into());
    queue.send(PeerId(1), packet(6, 2, 2).into());
    queue.send(PeerId(1), packet(6, 3, 2).into());
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.queued_bytes.load(Ordering::Relaxed), 80);
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 1);
    drop(rx.recv().await);
    assert_eq!(queue.bytes.available_permits(), 40);
    queue.send(PeerId(1), vec![0; 41].into());
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 2);
    drop(rx);
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 0);
    assert_eq!(queue.bytes.available_permits(), 80);
    let (tx, _rx) = mpsc::channel(1);
    let q = Queue {
        tx,
        packets: PacketBudget::new(4096),
        bytes: Arc::new(Semaphore::new(200)),
        metrics: Some(metrics.clone()),
    };
    q.send(PeerId(1), packet(6, 1, 2).into());
    q.send(PeerId(1), packet(6, 1, 2).into());
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 3);
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 1);
}
struct Allow;
impl ts_packetfilter::Filter for Allow {
    fn match_for(
        &self,
        _: &ts_packetfilter::PacketInfo,
        _: ts_packetfilter::filter::CapIter,
    ) -> Option<&str> {
        Some("all")
    }
}
struct Inspect;
impl Message<Inspect> for ForwarderActor {
    type Reply = (
        Selector,
        Vec<tokio::runtime::Handle>,
        Vec<mpsc::Sender<Control>>,
        ShardAdmission,
    );
    async fn handle(&mut self, _: Inspect, _: &mut Context<Self, Self::Reply>) -> Self::Reply {
        let g = self.shards.as_ref().unwrap();
        (
            g.dispatcher.selector.clone(),
            g.handles.clone(),
            g.controls.clone(),
            g.admission.clone(),
        )
    }
}
async fn setup() -> (
    Env,
    ActorRef<ForwarderActor>,
    OverlayFromDataplane,
    tokio::sync::watch::Sender<bool>,
) {
    let (_shutdown, rx) = tokio::sync::watch::channel(false);
    let mut env = Env::new(
        ts_keys::NodeState::generate(),
        rx,
        crate::env::ForwarderConfig::from_control_config(&ts_control::Config::default()),
    );
    env.forwarder_shards = 4;
    env.forward_all_ports = true;
    env.forward_routes = Arc::new(vec!["127.0.0.0/8".parse().unwrap()]);
    let (up, down) = if inline_egress() {
        let (tx, down) = mpsc::unbounded_channel();
        let (bounded, mut rx) = mpsc::channel(64);
        let fallback = tx.clone();
        tokio::spawn(async move {
            while let Some(batch) = rx.recv().await {
                if fallback.send(batch).is_err() {
                    break;
                }
            }
        });
        (OverlayToDataplane::for_probe(tx, Some(bounded)), down)
    } else {
        OverlayToDataplane::for_test(Default::default())
    };
    let (_input, input) = mpsc::unbounded_channel();
    let fwd = ForwarderActor::spawn((env.clone(), Default::default(), up, input));
    fwd.wait_for_startup_with_result(|r| r.is_ok()).await;
    let mut sources = ts_bart::Table::default();
    sources.insert("100.64.0.1/32".parse().unwrap(), PeerId(1));
    fwd.ask(SourceFilterState(Arc::new(sources))).await.unwrap();
    fwd.ask(PacketFilterState(Arc::new(Allow))).await.unwrap();
    (env, fwd, down, _shutdown)
}
/// Drive the production dispatcher, gated listeners and real OS streams on all four runtimes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_tcp_udp_and_both_half_closes_on_every_shard() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let (env, fwd, mut replies, _shutdown) = setup().await;
        let (selector, handles, _, admission) = fwd.ask(Inspect).await.unwrap();
        for (i, handle) in handles.iter().enumerate() {
            assert_eq!(
                handle
                    .spawn(async { std::thread::current().name().unwrap().to_owned() })
                    .await
                    .unwrap(),
                format!("ts-fwd-shard-{i}")
            );
        }
        let sink = fwd
            .ask(crate::forwarder_actor::ShardDispatch)
            .await
            .unwrap()
            .unwrap();
        let (
            mut peer,
            netstack::WakingPipe {
                rx: mut output,
                tx: input,
            },
        ) = crate::netstack_actor::piped_ingress_bounded(Default::default());
        let channel = peer.command_channel();
        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            peer.run_tokio().await;
        });
        channel
            .set_ips(["100.64.0.1".parse().unwrap()])
            .await
            .unwrap();
        tasks.spawn(async move {
            while let Some(p) = output.recv_async().await {
                let bytes = p.as_ref();
                if bytes[9] == 17 && bytes.len() > 100 {
                    let mut first = bytes[..100].to_vec();
                    first[2..4].copy_from_slice(&100u16.to_be_bytes());
                    first[6..8].copy_from_slice(&0x2000u16.to_be_bytes());
                    let mut last = bytes[..20].to_vec();
                    last.extend_from_slice(&bytes[100..]);
                    let len = last.len() as u16;
                    last[2..4].copy_from_slice(&len.to_be_bytes());
                    last[6..8].copy_from_slice(&10u16.to_be_bytes());
                    sink(PeerId(1), vec![last.into(), first.into()]);
                } else {
                    sink(PeerId(1), vec![crate::netstack_actor::into_packet(p)]);
                }
            }
        });
        tasks.spawn(async move {
            while let Some(batch) = replies.recv().await {
                for p in batch {
                    assert!(input.try_send(p.as_ref()));
                }
            }
        });
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tcp_addr = tcp.local_addr().unwrap();
        tasks.spawn(async move {
            loop {
                let (mut stream, _) = tcp.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = [0; 4];
                    stream.read_exact(&mut request).await.unwrap();
                    assert_eq!(&request, b"ping");
                    stream.write_all(b"pong").await.unwrap();
                    stream.shutdown().await.unwrap();
                    let mut tail = Vec::new();
                    stream.read_to_end(&mut tail).await.unwrap();
                    assert_eq!(&tail, b"tail");
                });
            }
        });
        let udp = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let udp_addr = udp.local_addr().unwrap();
        tasks.spawn(async move {
            let mut b = [0; 256];
            loop {
                let (n, addr) = udp.recv_from(&mut b).await.unwrap();
                udp.send_to(&b[..n], addr).await.unwrap();
            }
        });
        let _udp_reserved = admission
            .udp
            .clone()
            .acquire_many_owned(2044)
            .await
            .unwrap();
        for shard in 0..4 {
            let tcp_port = (10000..30000)
                .find(|p| selector.owner(PeerId(1), &packet(6, *p, tcp_addr.port())) == Some(shard))
                .unwrap();
            let udp_port = (30000..50000)
                .find(|p| {
                    selector.owner(PeerId(1), &packet(17, *p, udp_addr.port())) == Some(shard)
                })
                .unwrap();
            let mut stream = channel
                .tcp_connect(
                    SocketAddr::new("100.64.0.1".parse().unwrap(), tcp_port),
                    tcp_addr,
                )
                .await
                .unwrap();
            stream.write_all(b"ping").await.unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.unwrap();
            assert_eq!(&response, b"pong");
            // Remote EOF must preserve the opposite write half.
            stream.write_all(b"tail").await.unwrap();
            stream.shutdown().await.unwrap();
            let socket = channel
                .udp_bind(SocketAddr::new("100.64.0.1".parse().unwrap(), udp_port))
                .await
                .unwrap();
            let payload = [shard as u8 + 1; 128];
            socket.send_to(udp_addr, &payload).await.unwrap();
            let (from, bytes) = socket.recv_from_bytes().await.unwrap();
            assert_eq!(from, udp_addr);
            assert_eq!(&bytes[..], &payload);
        }
        assert_eq!(
            admission.udp.available_permits(),
            0,
            "four shards share one UDP cap"
        );
        let overcap = channel
            .udp_bind("100.64.0.1:61000".parse().unwrap())
            .await
            .unwrap();
        overcap.send_to(udp_addr, b"over-cap").await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), overcap.recv_from_bytes())
                .await
                .is_err()
        );
        // A peer EOF first must also allow a host reply after it.
        let after_eof = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = after_eof.local_addr().unwrap();
        tasks.spawn(async move {
            loop {
                let (mut stream, _) = after_eof.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    stream.read_to_end(&mut request).await.unwrap();
                    assert_eq!(&request, b"request");
                    stream.write_all(b"after-eof").await.unwrap();
                    stream.shutdown().await.unwrap();
                });
            }
        });
        for shard in 0..4 {
            let port = (50000..60000)
                .find(|p| selector.owner(PeerId(1), &packet(6, *p, addr.port())) == Some(shard))
                .unwrap();
            let mut stream = channel
                .tcp_connect(SocketAddr::new("100.64.0.1".parse().unwrap(), port), addr)
                .await
                .unwrap();
            stream.write_all(b"request").await.unwrap();
            stream.shutdown().await.unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.unwrap();
            assert_eq!(&response, b"after-eof");
        }
        // Exercise the shared TCP cap with four simultaneously held host streams, then a
        // fifth flow on an otherwise roomy listener. Overlay handshake succeeds before dial.
        while admission.tcp.available_permits() != 2048 {
            tokio::task::yield_now().await;
        }
        let _tcp_reserved = admission
            .tcp
            .clone()
            .acquire_many_owned(2044)
            .await
            .unwrap();
        let holding = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = holding.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        tasks.spawn(async move {
            let mut held = Vec::new();
            loop {
                let (stream, _) = holding.accept().await.unwrap();
                held.push(stream);
                count.fetch_add(1, Ordering::SeqCst);
            }
        });
        let mut clients = Vec::new();
        for shard in 0..4 {
            let port = (20000..30000)
                .find(|p| selector.owner(PeerId(1), &packet(6, *p, addr.port())) == Some(shard))
                .unwrap();
            clients.push(
                channel
                    .tcp_connect(SocketAddr::new("100.64.0.1".parse().unwrap(), port), addr)
                    .await
                    .unwrap(),
            );
        }
        while accepted.load(Ordering::SeqCst) != 4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(admission.tcp.available_permits(), 0);
        let _overcap = channel
            .tcp_connect("100.64.0.1:62000".parse().unwrap(), addr)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            accepted.load(Ordering::SeqCst),
            4,
            "N must not multiply the node TCP cap"
        );
        fwd.kill();
        fwd.wait_for_shutdown().await;
        for handle in handles {
            while handle.spawn(async {}).await.is_ok() {
                tokio::task::yield_now().await;
            }
        }
        env.bus.kill();
        env.bus.wait_for_shutdown().await;
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn security_updates_ack_every_shard_in_order_and_panic_stops_global_forwarding() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (env, fwd, _replies, shutdown) = setup().await;
        let (_, handles, controls, _) = fwd.ask(Inspect).await.unwrap();
        let dataplane = crate::dataplane::DataplaneActor::spawn(env.clone());
        let mut guard =
            crate::security::SecurityGuard::new(shutdown.clone(), env.supervisor_runtime.clone());
        guard.watch(&env.bus);
        guard.watch(&fwd);
        guard.watch(&dataplane);
        guard.ready(&dataplane).await.unwrap();
        let (id, ..) = dataplane
            .ask(crate::dataplane::NewOverlayTransport)
            .await
            .unwrap();
        let sink = fwd
            .ask(crate::forwarder_actor::ShardDispatch)
            .await
            .unwrap()
            .unwrap();
        let owners = fwd
            .ask(crate::forwarder_actor::ShardOwnership)
            .await
            .unwrap()
            .unwrap();
        dataplane
            .ask(crate::dataplane::AuthenticatedForwarder {
                id,
                sink,
                forwarder: fwd.clone(),
                owners,
            })
            .await
            .unwrap();
        let pump = dataplane
            .ask(crate::dataplane::PacketPumpAbortHandle)
            .await
            .unwrap();
        guard.packet_pump(pump.clone());
        for _ in 0..20 {
            fwd.ask(PacketFilterState(Arc::new(ts_packetfilter::DropAllFilter)))
                .await
                .unwrap();
            for tx in &controls {
                let (ack, rx) = oneshot::channel();
                tx.send(Control::Probe(ack)).await.unwrap();
                assert!(!rx.await.unwrap().0);
            }
            fwd.ask(PacketFilterState(Arc::new(Allow))).await.unwrap();
            for tx in &controls {
                let (ack, rx) = oneshot::channel();
                tx.send(Control::Probe(ack)).await.unwrap();
                assert!(rx.await.unwrap().0);
            }
        }
        // Use the ordered guaranteed bus too, with an ask after publication as a barrier.
        env.publish(PacketFilterState(Arc::new(ts_packetfilter::DropAllFilter)))
            .await
            .unwrap();
        env.bus
            .ask(kameo_actors::message_bus::Unregister::<u64>::new(fwd.id()))
            .await
            .unwrap();
        dataplane
            .ask(crate::dataplane::PacketPumpAbortHandle)
            .await
            .unwrap();
        for tx in &controls {
            let (ack, rx) = oneshot::channel();
            tx.send(Control::Probe(ack)).await.unwrap();
            assert!(!rx.await.unwrap().0);
        }
        // Each independent control worker is live and acknowledges; a panic of any of them
        // must become a shard exit, kill this parent and cancel all runtime tasks.
        controls[2].send(Control::Panic).await.unwrap();
        fwd.wait_for_shutdown().await;
        for handle in handles {
            while handle.spawn(async {}).await.is_ok() {
                tokio::task::yield_now().await;
            }
        }
        dataplane.wait_for_shutdown().await;
        env.bus.wait_for_shutdown().await;
        assert!(*shutdown.borrow());
        while !pump.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn source_reassignment_cannot_inherit_old_sockets() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (env, fwd, _replies, _shutdown) = setup().await;
        let (_, _, controls, _) = fwd.ask(Inspect).await.unwrap();
        let sink = fwd
            .ask(crate::forwarder_actor::ShardDispatch)
            .await
            .unwrap()
            .unwrap();
        sink(PeerId(1), vec![packet(17, 10000, 9).into()]);
        loop {
            let mut used = 0;
            for tx in &controls {
                let (ack, rx) = oneshot::channel();
                tx.send(Control::Probe(ack)).await.unwrap();
                used += rx.await.unwrap().1;
            }
            if used > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        let mut reassigned = ts_bart::Table::default();
        reassigned.insert("100.64.0.1/32".parse().unwrap(), PeerId(2));
        drop(fwd.ask(SourceFilterState(Arc::new(reassigned))).await);
        fwd.wait_for_shutdown().await;
        env.bus.kill();
        env.bus.wait_for_shutdown().await;
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn queued_packets_revalidate_before_the_gate() {
    let security = Arc::new(Mutex::new(Security {
        sources: SourceFilterState(Arc::new(Default::default())),
        filter: PacketFilterState(Arc::new(Allow)),
    }));
    let (tx, rx) = mpsc::channel(8);
    let metrics = Arc::new(Metrics::default());
    let queue = Queue {
        tx,
        packets: PacketBudget::new(4096),
        bytes: Arc::new(Semaphore::new(1000)),
        metrics: Some(metrics.clone()),
    };
    queue.send(PeerId(1), packet(6, 1, 443).into());
    drop(queue);
    let owners = Arc::new(Mutex::new(HashMap::new()));
    pump(
        rx,
        security,
        Some(metrics.clone()),
        owners.clone(),
        |_| panic!("revoked queued packet reached gate"),
        || {},
    )
    .await;
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 1);
    assert!(owners.lock().unwrap().is_empty());
}
#[tokio::test]
async fn public_runtime_drop_joins_all_opt_in_shard_threads() {
    let control = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ts_control::Config {
        server_url: format!("http://{}/", control.local_addr().unwrap())
            .parse()
            .unwrap(),
        ..Default::default()
    };
    let runtime = crate::Runtime::spawn(config, None, ts_keys::NodeState::generate())
        .await
        .unwrap();
    let handles = runtime.env.forwarder_handles.as_ref().clone();
    assert_eq!(
        handles.len(),
        if count_from_env().unwrap() > 1 {
            count_from_env().unwrap()
        } else {
            0
        }
    );
    drop(runtime);
    for handle in handles {
        assert!(handle.spawn(async {}).await.unwrap_err().is_cancelled());
    }
}
#[test]
fn destination_guard_and_operator_dns_exception_precede_all_shards() {
    let exit = ts_forwarder::FlowClass::ExitNode;
    let subnet = ts_forwarder::FlowClass::Subnet;
    let private = "192.168.1.1".parse().unwrap();
    let public = "8.8.8.8".parse().unwrap();
    let dns = packet(17, 12345, 53);
    let tcp = packet(6, 12345, 443);
    assert!(!destination_allowed(exit, private, &dns, false));
    assert!(!destination_allowed(exit, private, &tcp, true));
    assert!(destination_allowed(exit, private, &dns, true));
    assert!(destination_allowed(subnet, private, &tcp, false));
    assert!(destination_allowed(exit, public, &tcp, false));
}

#[tokio::test]
async fn batched_queue_keeps_packet_byte_caps_order_and_drop_accounting() {
    let (tx, mut rx) = mpsc::channel(8);
    let metrics = Arc::new(Metrics::default());
    let queue = Queue {
        tx,
        packets: PacketBudget::new(3),
        bytes: Arc::new(Semaphore::new(120)),
        metrics: Some(metrics.clone()),
    };
    queue.send_batch(
        PeerId(7),
        (1..=4).map(|port| packet(6, port, 443).into()).collect(),
    );
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 3);
    assert_eq!(metrics.queued_bytes.load(Ordering::Relaxed), 120);
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 1);
    let batch = rx.recv().await.unwrap();
    let mut ports = Vec::new();
    for queued in batch {
        assert_eq!(queued.peer, PeerId(7));
        ports.push(u16::from_be_bytes(
            queued.packet.as_ref().unwrap().as_ref()[20..22]
                .try_into()
                .unwrap(),
        ));
    }
    assert_eq!(ports, [1, 2, 3]);
    assert_eq!(queue.packets.available_permits(), 3);
    assert_eq!(queue.bytes.available_permits(), 120);
    // Independently exercise the packet cap with byte space remaining.
    queue.send_batch(PeerId(7), (0..4).map(|_| vec![0; 1].into()).collect());
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 3);
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 2);
    drop(rx);
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 0);
    assert_eq!(queue.packets.available_permits(), 3);
    assert_eq!(queue.bytes.available_permits(), 120);
}

#[tokio::test]
async fn batched_ingress_revalidates_after_revocation_at_turn_boundary() {
    let mut sources = ts_bart::Table::default();
    sources.insert("100.64.0.1/32".parse().unwrap(), PeerId(1));
    let security = Arc::new(Mutex::new(Security {
        sources: SourceFilterState(Arc::new(sources)),
        filter: PacketFilterState(Arc::new(Allow)),
    }));
    let (tx, rx) = mpsc::channel(8);
    let metrics = Arc::new(Metrics::default());
    let queue = Queue {
        tx,
        packets: PacketBudget::new(192),
        bytes: Arc::new(Semaphore::new(192 * 40)),
        metrics: Some(metrics.clone()),
    };
    for _ in 0..3 {
        queue.send_batch(
            PeerId(1),
            (0..64).map(|_| packet(6, 1, 443).into()).collect(),
        );
    }
    drop(queue);
    let (ready, notified) = oneshot::channel();
    let revoke_security = security.clone();
    let revoke = tokio::spawn(async move {
        notified.await.unwrap();
        revoke_security.lock().unwrap().sources = SourceFilterState(Arc::new(Default::default()));
    });
    let mut admitted = 0;
    let mut ready = Some(ready);
    pump(
        rx,
        security,
        Some(metrics.clone()),
        Arc::new(Mutex::new(HashMap::new())),
        |_| {
            admitted += 1;
            if admitted == 128 {
                ready.take().unwrap().send(()).unwrap();
            }
            true
        },
        || {},
    )
    .await;
    revoke.await.unwrap();
    assert_eq!(admitted, 128);
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 64);
    assert_eq!(metrics.queued.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn inline_egress_full_mailbox_keeps_fifo_then_resumes_and_closed_fails() {
    let (tx, _unused) = mpsc::unbounded_channel();
    let (bounded, mut receiver) = mpsc::channel(1);
    let up = OverlayToDataplane::for_probe(tx, Some(bounded));
    let lagging = Arc::new(AtomicBool::new(false));
    let mut sink = crate::forwarder_actor::inline_sink(up.clone(), lagging.clone(), None, false);
    let bytes = |n| vec![bytes::Bytes::from(vec![n])];
    sink(bytes(0)).unwrap();
    let first = sink(bytes(1)).unwrap_err();
    assert!(lagging.load(Ordering::Relaxed));
    assert_eq!(receiver.recv().await.unwrap()[0].as_ref(), &[0]);
    // Capacity is free, but an older failed publication still belongs to the fallback pump.
    let second = sink(bytes(2)).unwrap_err();
    up.send_wait_counted(
        first
            .into_iter()
            .map(crate::netstack_actor::into_packet)
            .collect(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(receiver.recv().await.unwrap()[0].as_ref(), &[1]);
    up.send_wait_counted(
        second
            .into_iter()
            .map(crate::netstack_actor::into_packet)
            .collect(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(receiver.recv().await.unwrap()[0].as_ref(), &[2]);
    // The colocated pump clears this only after its current send and all queued tails finish.
    lagging.store(false, Ordering::Relaxed);
    sink(bytes(3)).unwrap();
    assert_eq!(receiver.recv().await.unwrap()[0].as_ref(), &[3]);
    drop(receiver);
    let closed = sink(bytes(4)).unwrap_err();
    assert!(
        up.send_wait_counted(
            closed
                .into_iter()
                .map(crate::netstack_actor::into_packet)
                .collect(),
            None
        )
        .await
        .is_err()
    );
}
