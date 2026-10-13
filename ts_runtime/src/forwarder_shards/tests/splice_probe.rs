//! Opt-in local diagnostic: dispatcher/gate/stack/host splice, optional WG, no control or UDP.
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
    let wg = std::env::var("TS_BENCH_SPLICE_WG").as_deref() == Ok("1");
    let keys = (ts_tunnel::NodeKeyPair::new(), ts_tunnel::NodeKeyPair::new());
    let node_dp = Arc::new(ts_dataplane::async_tokio::DataPlane::new(keys.0.clone()));
    let peer_dp = Arc::new(ts_dataplane::async_tokio::DataPlane::new(keys.1.clone()));
    let (node_overlay, node_tx, _) = node_dp.new_overlay_transport().await;
    let (peer_overlay, peer_tx, peer_rx) = peer_dp.new_overlay_transport().await;
    let (up, mut replies) = if wg {
        (
            OverlayToDataplane::for_probe(
                node_tx,
                if n > 1 {
                    Some(node_dp.bounded_forwarder_egress(64).await)
                } else {
                    None
                },
            ),
            peer_rx,
        )
    } else {
        OverlayToDataplane::for_test(Default::default())
    };
    let (legacy, input) = mpsc::unbounded_channel();
    let config = netstack::netcore::Config {
        mtu: std::env::var("TS_BENCH_SPLICE_MTU")
            .unwrap_or("1280".into())
            .parse()
            .unwrap(),
        tcp_buffer_size: std::env::var("TS_BENCH_SPLICE_BUFFER")
            .unwrap_or("262144".into())
            .parse()
            .unwrap(),
        ..Default::default()
    };
    let fwd = ForwarderActor::spawn((env.clone(), config.clone(), up, input));
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
    let harness_peer = std::env::var("TS_BENCH_SPLICE_PEER_MODE").as_deref() == Ok("harness");
    let peer_pipe = if harness_peer {
        // synthetic_peer's driver: unbounded pipe, full RX/TX work budget and per-packet output.
        let (mut peer, pipe) = netstack::piped(config.clone());
        peer.set_work_budget(Some(128));
        (peer, pipe)
    } else {
        crate::netstack_actor::piped_ingress_bounded(config.clone())
    };
    let (
        mut peer,
        netstack::WakingPipe {
            rx: mut output,
            tx: input,
        },
    ) = peer_pipe;
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
    if wg {
        for (dp, key, overlay, source) in [
            (&node_dp, keys.1.public, node_overlay, "100.64.0.1/32"),
            (&peer_dp, keys.0.public, peer_overlay, "127.0.0.0/8"),
        ] {
            let mut state = dp.inner().await;
            state.wireguard.upsert_peer(
                ts_tunnel::PeerId(1),
                ts_tunnel::PeerConfig {
                    key,
                    psk: [0; 32].into(),
                    persistent_keepalive_interval: None,
                },
            );
            let mut inbound = ts_bart::Table::default();
            inbound.insert(
                "0.0.0.0/0".parse().unwrap(),
                ts_overlay_router::inbound::RouteAction::ToOverlay(overlay),
            );
            state.or_in.swap(inbound);
            let mut outbound = ts_bart::Table::default();
            outbound.insert(
                "0.0.0.0/0".parse().unwrap(),
                ts_overlay_router::outbound::RouteAction::Wireguard(PeerId(1)),
            );
            state.or_out.swap(outbound);
            let mut sources = ts_bart::Table::default();
            sources.insert(source.parse().unwrap(), PeerId(1));
            state.src_filter_in = Arc::new(sources);
            state.packet_filter = Arc::new(Allow);
        }
        {
            let mut state = node_dp.inner().await;
            state.authenticated_overlay = Some((node_overlay, sink.clone()));
            state.forwarder_source_owners = fwd
                .ask(crate::forwarder_actor::ShardOwnership)
                .await
                .unwrap();
        }
        let (node_underlay, mut node_output, node_input) = node_dp.new_underlay_transport().await;
        let (peer_underlay, mut peer_output, peer_input) = peer_dp.new_underlay_transport().await;
        node_dp
            .inner()
            .await
            .ur_out
            .table
            .insert(PeerId(1), node_underlay);
        peer_dp
            .inner()
            .await
            .ur_out
            .table
            .insert(PeerId(1), peer_underlay);
        tasks.spawn(async move {
            while let Some((_, p)) = node_output.recv().await {
                peer_input.send((PeerId(1), p)).unwrap();
            }
        });
        tasks.spawn(async move {
            while let Some((_, p)) = peer_output.recv().await {
                node_input.send((PeerId(1), p)).unwrap();
            }
        });
        tasks.spawn(async move {
            node_dp.run().await;
        });
        tasks.spawn(async move {
            if harness_peer {
                loop {
                    peer_dp.step().await;
                    tokio::task::yield_now().await;
                }
            } else {
                peer_dp.run().await;
            }
        });
    }
    tasks.spawn(async move {
        while let Some(p) = output.recv_async().await {
            let mut batch = vec![crate::netstack_actor::into_packet(p)];
            while batch.len() < 64
                && let Some(p) = output.try_recv()
            {
                batch.push(crate::netstack_actor::into_packet(p));
            }
            if wg {
                peer_tx.send(batch).unwrap();
            } else {
                sink(PeerId(1), batch);
            }
            if harness_peer {
                tokio::task::yield_now().await;
            }
        }
    });
    let packet_wakes =
        harness_peer || std::env::var("TS_BENCH_SPLICE_PEER_WAKE").as_deref() == Ok("packet");
    tasks.spawn(async move {
        while let Some(batch) = replies.recv().await {
            for p in batch {
                let accepted = if packet_wakes {
                    input.try_send(p.as_ref())
                } else {
                    input.try_send_deferred(p.as_ref())
                };
                if !accepted {
                    drops.fetch_add(1, Ordering::Relaxed);
                }
            }
            if !packet_wakes {
                input.notify_receive();
            }
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
    let load_echo = std::env::var("TS_BENCH_SPLICE_LOAD_ECHO").as_deref() == Ok("1");
    let mut echo = Some(echo);
    if !load_echo {
        echo.as_mut().unwrap().shutdown().await.unwrap();
        echo = None;
    }
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
        let cpu_before = process_cpu();
        if crate::pipeline_metrics::enabled() {
            println!(
                "PROBE_METRICS before mode={} {:?}",
                mode as char,
                crate::pipeline_metrics::snapshot()
            );
        }
        let start = Instant::now();
        let (stop_echo, mut echo_done) = tokio::sync::watch::channel(false);
        let echo_task = echo.take().map(|mut stream| {
            tokio::spawn(async move {
                let mut timings = Vec::new();
                loop {
                    tokio::select! {
                        _ = echo_done.changed() => break,
                        _ = tokio::time::sleep(Duration::from_millis(1)) => {}
                    }
                    let start = Instant::now();
                    stream.write_all(&[5; 64]).await.unwrap();
                    let mut reply = [0; 64];
                    stream.read_exact(&mut reply).await.unwrap();
                    assert_eq!(reply, [5; 64]);
                    timings.push(start.elapsed().as_secs_f64() * 1e6);
                }
                (stream, timings)
            })
        });
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
        stop_echo.send_replace(true);
        if let Some(task) = echo_task {
            let (stream, mut timings) = task.await.unwrap();
            echo = Some(stream);
            timings.sort_by(f64::total_cmp);
            if !timings.is_empty() {
                println!(
                    "PROBE_LOAD_ECHO n={n} mode={} samples={} p50={:.1} p99={:.1}",
                    mode as char,
                    timings.len(),
                    timings[timings.len() / 2],
                    timings[timings.len() * 99 / 100]
                );
            }
        }
        let after = ts_forwarder::splice_metrics::snapshot();
        let cpu = process_cpu() - cpu_before;
        println!(
            "PROBE_CPU n={n} wg={wg} batch={} mode={} cpu_s={cpu:.4} cpu_s_per_gb={:.3}",
            batching(),
            mode as char,
            cpu / (bytes * flows) as f64 * 1e9
        );
        if crate::pipeline_metrics::enabled() {
            println!(
                "PROBE_METRICS after mode={} {:?}",
                mode as char,
                crate::pipeline_metrics::snapshot()
            );
        }
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
    if let Some(mut echo) = echo {
        echo.shutdown().await.unwrap();
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

#[cfg(unix)]
fn process_cpu() -> f64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid timespec; measures all threads, including the local generator and sink.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut t) },
        0
    );
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

#[cfg(not(unix))]
fn process_cpu() -> f64 {
    0.0
}
