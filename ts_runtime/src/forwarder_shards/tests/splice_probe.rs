//! Opt-in local diagnostic: production dispatcher/gate/stack/host splice, without WG/control.
use std::time::Instant;

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "local measurement; run alone with --ignored --nocapture"]
async fn local_splice_probe() {
    tokio::time::timeout(Duration::from_secs(120), probe())
        .await
        .unwrap();
}

async fn probe() {
    let n = count_from_env().unwrap();
    let flows: usize = std::env::var("TS_BENCH_SPLICE_FLOWS")
        .unwrap_or("128".into())
        .parse()
        .unwrap();
    let bytes: usize = std::env::var("TS_BENCH_SPLICE_BYTES")
        .unwrap_or("4000000".into())
        .parse()
        .unwrap();
    let (shutdown, rx) = tokio::sync::watch::channel(false);
    let mut env = Env::new(
        ts_keys::NodeState::generate(),
        rx,
        crate::env::ForwarderConfig::from_control_config(&ts_control::Config::default()),
    );
    env.forwarder_shards = n;
    env.forward_all_ports = true;
    env.forward_routes = Arc::new(vec!["127.0.0.0/8".parse().unwrap()]);
    let (up, mut replies) = OverlayToDataplane::for_test(Default::default());
    let (legacy, input) = mpsc::unbounded_channel();
    let fwd = ForwarderActor::spawn((env.clone(), Default::default(), up, input));
    fwd.wait_for_startup_with_result(|r| r.is_ok()).await;
    let mut sources = ts_bart::Table::default();
    sources.insert("100.64.0.1/32".parse().unwrap(), PeerId(1));
    fwd.ask(SourceFilterState(Arc::new(sources))).await.unwrap();
    fwd.ask(PacketFilterState(Arc::new(Allow))).await.unwrap();
    let sink: ts_dataplane::AuthenticatedOverlaySink = if n > 1 {
        fwd.ask(crate::forwarder_actor::ShardDispatch)
            .await
            .unwrap()
            .unwrap()
    } else {
        Arc::new(move |_, packets| {
            legacy.send(packets).unwrap();
        })
    };
    let (
        mut peer,
        netstack::WakingPipe {
            rx: mut output,
            tx: input,
        },
    ) = crate::netstack_actor::piped_ingress_bounded(Default::default());
    let channel = peer.command_channel();
    let peer_drops = Arc::new(AtomicU64::new(0));
    let drops = peer_drops.clone();
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
            let mut batch = vec![crate::netstack_actor::into_packet(p)];
            while batch.len() < 64
                && let Some(p) = output.try_recv()
            {
                batch.push(crate::netstack_actor::into_packet(p));
            }
            sink(PeerId(1), batch);
        }
    });
    tasks.spawn(async move {
        while let Some(batch) = replies.recv().await {
            for p in batch {
                if !input.try_send_deferred(p.as_ref()) {
                    drops.fetch_add(1, Ordering::Relaxed);
                }
            }
            input.notify_receive();
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let echo_owner = if n > 1 {
        fwd.ask(Inspect)
            .await
            .unwrap()
            .0
            .owner(PeerId(1), &packet(6, 10000, addr.port()))
            .unwrap()
    } else {
        0
    };
    let (done, mut finished) = mpsc::unbounded_channel();
    tasks.spawn(async move {
        let mut sockets = JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let done = done.clone();
                    sockets.spawn(async move {
                        stream.set_nodelay(true).unwrap();
                        let mode = stream.read_u8().await.unwrap();
                        if mode == b'e' {
                            let mut buf = [0; 64];
                            while stream.read_exact(&mut buf).await.is_ok() {
                                stream.write_all(&buf).await.unwrap();
                            }
                        } else if mode == b'u' {
                            let mut got = Vec::new();
                            stream.read_to_end(&mut got).await.unwrap();
                            assert_eq!(got.len(), bytes);
                            assert!(got.iter().all(|b| *b == 7));
                            stream.shutdown().await.unwrap();
                            done.send(()).unwrap();
                        } else {
                            assert_eq!(mode, b'd');
                            stream.write_all(&vec![7; bytes]).await.unwrap();
                            stream.shutdown().await.unwrap();
                        }
                    });
                }
                joined = sockets.join_next(), if !sockets.is_empty() => { joined.unwrap().unwrap(); }
            }
        }
    });
    let mut port = 10000u16;
    let mut echo = channel
        .tcp_connect(SocketAddr::new("100.64.0.1".parse().unwrap(), port), addr)
        .await
        .unwrap();
    echo.write_u8(b'e').await.unwrap();
    let mut rtts = Vec::new();
    for i in 0..1100 {
        let start = Instant::now();
        echo.write_all(&[5; 64]).await.unwrap();
        let mut reply = [0; 64];
        echo.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, [5; 64]);
        if i >= 100 {
            rtts.push(start.elapsed().as_secs_f64() * 1e6);
        }
    }
    echo.shutdown().await.unwrap();
    drop(echo);
    rtts.sort_by(f64::total_cmp);
    println!(
        "PROBE n={n} echo_owner={echo_owner} echo_us p50={:.1} p90={:.1} p99={:.1} max={:.1}",
        rtts[500], rtts[900], rtts[990], rtts[999]
    );
    for mode in [b'u', b'd'] {
        let direction = std::env::var("TS_BENCH_SPLICE_DIRECTION").unwrap_or_default();
        if (direction == "up" && mode != b'u') || (direction == "down" && mode != b'd') {
            continue;
        }
        let before = ts_forwarder::splice_metrics::snapshot();
        let start = Instant::now();
        let mut clients = JoinSet::new();
        for _ in 0..flows {
            port += 1;
            let channel = channel.clone();
            let local = SocketAddr::new("100.64.0.1".parse().unwrap(), port);
            clients.spawn(async move {
                let mut stream = channel.tcp_connect(local, addr).await.unwrap();
                stream.write_u8(mode).await.unwrap();
                if mode == b'u' {
                    stream.write_all(&vec![7; bytes]).await.unwrap();
                    stream.shutdown().await.unwrap();
                    let mut tail = Vec::new();
                    stream.read_to_end(&mut tail).await.unwrap();
                    assert!(tail.is_empty());
                } else {
                    let mut got = Vec::new();
                    stream.read_to_end(&mut got).await.unwrap();
                    assert_eq!(got.len(), bytes);
                    assert!(got.iter().all(|b| *b == 7));
                    stream.shutdown().await.unwrap();
                }
            });
        }
        while let Some(joined) = clients.join_next().await {
            joined.unwrap();
        }
        if mode == b'u' {
            for _ in 0..flows {
                finished.recv().await.unwrap();
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        let after = ts_forwarder::splice_metrics::snapshot();
        let hist = core::array::from_fn::<_, 9, _>(|i| {
            after.host_write_sizes[i] - before.host_write_sizes[i]
        });
        let count: u64 = hist.iter().sum();
        println!(
            "PROBE n={n} mode={} flows={flows} bytes={} seconds={elapsed:.3} mbps={:.1} mean_write={:.1} writes={hist:?} pending={} partial={}",
            mode as char,
            bytes * flows,
            bytes as f64 * flows as f64 * 8.0 / elapsed / 1e6,
            (after.host_write_bytes - before.host_write_bytes) as f64 / count.max(1) as f64,
            after.host_write_pending - before.host_write_pending,
            after.host_write_partial - before.host_write_partial
        );
    }
    fwd.kill();
    fwd.wait_for_shutdown().await;
    env.bus.kill();
    env.bus.wait_for_shutdown().await;
    drop(shutdown);
    tasks.shutdown().await;
    println!(
        "PROBE peer_ingress_drops={}",
        peer_drops.load(Ordering::Relaxed)
    );
}
