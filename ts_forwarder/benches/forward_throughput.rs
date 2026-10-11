//! Exit-node forwarder throughput, isolated from WireGuard and the underlay.
//!
//! A "peer" netstack dials an exit-node destination over an in-memory pipe (overlay MTU 1280); the
//! forwarder netstack accepts it, classifies it as `0.0.0.0/0`, dials a real loopback socket through
//! `HostExitDialer`'s socket path and splices. Measures bulk TCP in each direction across N parallel flows.
//!
//! `RTT_MS` inserts a symmetric one-way delay of `RTT_MS / 2` on the overlay pipe, standing in for
//! the peer's path latency (the forwarder's own egress leg stays on loopback).
//!
//! ```sh
//! `TCP_BUF_KB` overrides the per-direction netstack TCP buffer (both stacks).
//!
//! RTT_MS=40 cargo bench -p geiserx_ts_forwarder --bench forward_throughput -- [secs] [flows] [up|down]
//! ```

use core::net::{Ipv4Addr, SocketAddr};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ts_forwarder::{DialError, DialedUdp, FlowClass, Forwarder, HostExitDialer, RealDialer, RouteTable};
use ts_netstack_smoltcp::{
    CreateSocket, Netstack, WakingPipe, WakingPipeDev, WakingPipeReceiver, WakingPipeSender,
    netcore::{self, Channel, HasChannel, NetstackControl, smoltcp},
};

const PEER_IP: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 1);
const OVERLAY_MTU: usize = 1280;
const CHUNK: usize = 64 * 1024;

/// Bytes the loopback server actually received (`up`), to cross-check the sender-side count.
static SERVER_RX: AtomicU64 = AtomicU64::new(0);

/// Forward packets from `rx` to `tx`, each delivered `delay` after it was received.
fn spawn_delay(mut rx: WakingPipeReceiver, tx: WakingPipeSender, delay: Duration) {
    let (q_tx, mut q_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(pkt) = rx.recv_async().await {
            if q_tx.send((tokio::time::Instant::now() + delay, pkt)).is_err() {
                return;
            }
        }
    });
    tokio::spawn(async move {
        while let Some((due, pkt)) = q_rx.recv().await {
            tokio::time::sleep_until(due).await;
            tx.send_async(&pkt).await;
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

async fn spawn_pair(rtt: Duration, tcp_buf: Option<usize>) -> (Channel, Channel) {
    let mut config = netcore::Config {
        mtu: OVERLAY_MTU,
        ..netcore::Config::default()
    };
    if let Some(b) = tcp_buf {
        config.tcp_buffer_size = b;
    }
    let (p1, p2) = if rtt.is_zero() {
        WakingPipe::new(None)
    } else {
        let (p1, a) = WakingPipe::new(None);
        let (b, p2) = WakingPipe::new(None);
        spawn_delay(a.rx, b.tx, rtt / 2);
        spawn_delay(b.rx, a.tx, rtt / 2);
        (p1, p2)
    };
    let dev = |pipe| WakingPipeDev {
        pipe,
        mtu: OVERLAY_MTU,
        medium: smoltcp::phy::Medium::Ip,
    };

    let mut peer = Netstack::new(dev(p1), config.clone());
    let mut fwd = Netstack::new(dev(p2), config);
    let peer_ch = peer.command_channel();
    let fwd_ch = fwd.command_channel();
    tokio::spawn(async move { peer.run_tokio().await });
    tokio::spawn(async move { fwd.run_tokio().await });

    peer_ch.set_ips([PEER_IP.into()]).await.unwrap();
    fwd_ch.set_any_ip(true).await.unwrap();
    (peer_ch, fwd_ch)
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

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--bench").collect();
    let secs: f64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(5.0);
    let flows: u16 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let down = args.get(2).is_some_and(|a| a == "down");

    let rtt_ms: u64 = std::env::var("RTT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);

    let tcp_buf_kb: Option<usize> = std::env::var("TCP_BUF_KB").ok().and_then(|v| v.parse().ok());

    let server = spawn_server(down).await;
    let (peer_ch, fwd_ch) =
        spawn_pair(Duration::from_millis(rtt_ms), tcp_buf_kb.map(|k| k * 1024)).await;
    let forwarder = Forwarder::new(
        fwd_ch,
        RouteTable::new(["0.0.0.0/0".parse().unwrap()]),
        LoopbackInternetDialer,
        vec![server.port()],
        vec![],
    );
    tokio::spawn(async move {
        let _ = forwarder.run().await;
    });

    let deadline = Instant::now() + Duration::from_secs_f64(secs);
    let mut tasks = Vec::new();
    for i in 0..flows {
        let peer_ch = peer_ch.clone();
        tasks.push(tokio::spawn(async move {
            let local = SocketAddr::new(PEER_IP.into(), 10_000 + i);
            let mut s = loop {
                match peer_ch.tcp_connect(local, server).await {
                    Ok(s) => break s,
                    Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            };
            let mut buf = vec![0u8; CHUNK];
            let mut bytes = 0u64;
            while Instant::now() < deadline {
                let n = if down {
                    s.read(&mut buf).await.unwrap()
                } else {
                    s.write_all(&buf).await.unwrap();
                    CHUNK
                };
                bytes += n as u64;
            }
            bytes
        }));
    }
    let start = Instant::now();
    let mut total = 0u64;
    for t in tasks {
        total += t.await.unwrap();
    }
    let elapsed = start.elapsed().as_secs_f64();
    let mbps = |b: u64| b as f64 * 8.0 / elapsed / 1e6;
    println!(
        "dir={} rtt_ms={rtt_ms} tcp_buf_kb={} flows={flows} secs={elapsed:.2} mbps={:.1} server_rx_mbps={:.1}",
        if down { "down" } else { "up" },
        tcp_buf_kb.map_or("default".to_owned(), |k| k.to_string()),
        mbps(total),
        mbps(SERVER_RX.load(Ordering::Relaxed)),
    );
}
