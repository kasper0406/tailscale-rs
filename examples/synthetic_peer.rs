//! Independent WireGuard/TCP load peer. No control client or Go process.
//! Keys and source IP must be authorized before targeting a production node.
//! `fixture` is a loopback-only, isolated forwarder endpoint, never an exit-node runner.
use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
    task::JoinSet,
};
use ts_forwarder::{
    DialError, DialedUdp, FlowClass, Forwarder, HostExitDialer, RealDialer, RouteTable,
};
use ts_keys::{DiscoPrivateKey, DiscoPublicKey, NodePrivateKey, NodePublicKey};
use ts_netstack_smoltcp::{
    CreateSocket, HasChannel, Netstack, WakingPipeReceiver, WakingPipeSender,
    netcore::{self, NetstackControl},
};
use ts_packet::PacketMut;
use ts_tunnel::{Endpoint, PeerConfig, PeerId};
use zerocopy::IntoBytes;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 4)]
    workers: usize,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Create a fresh private-key file (0600, create_new); print only its public key.
    Keygen { file: PathBuf },
    /// Enroll a distinct benchmark identity once. Does not advertise routes or run load.
    Enroll {
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long)]
        disco_key_file: PathBuf,
        #[arg(long)]
        identity_file: PathBuf,
        #[arg(long)]
        /// Auth key file. Without one, print control's login URL and wait (up to 1 h) for it to
        /// be approved in a browser.
        auth_key_file: Option<PathBuf>,
        #[arg(long, default_value = "synthetic-load")]
        hostname: String,
        #[arg(long)]
        target: Option<String>,
    },
    /// Live-capable peer: admitted static key, IP and public destination are mandatory.
    Peer {
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long)]
        node_key: String,
        #[arg(long, requires = "node_disco_key")]
        disco_key_file: Option<PathBuf>,
        #[arg(long, requires = "disco_key_file")]
        node_disco_key: Option<String>,
        #[arg(long)]
        endpoint: SocketAddr,
        #[arg(long)]
        source: Ipv4Addr,
        #[arg(long)]
        destination: SocketAddr,
        #[arg(long, default_value_t = 8)]
        flows: usize,
        /// Report readiness and wait for GO on stdin (matrix orchestration only).
        #[arg(long)]
        wait_for_start: bool,
        /// Use the existing exit_node app sink (infinite d stream), rather than the finite lab sink.
        #[arg(long)]
        legacy_stream: bool,
        #[arg(long, default_value_t = 500_000_000)]
        bytes: u64,
        #[arg(long, default_value="down", value_parser=["down", "up"])]
        direction: String,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        #[arg(long, default_value_t = 256)]
        tcp_buf_kb: usize,
        /// Enrolled identity (from `enroll`). With `--advertise`, hold a control session for the
        /// run and report that address as this peer's endpoint, so the node learns it from the
        /// netmap (the node never trusts a private source address learned from disco alone).
        #[arg(long, requires_all = ["advertise", "bind"])]
        identity_file: Option<PathBuf>,
        /// Local UDP bind for the WireGuard socket (default: an ephemeral port).
        #[arg(long)]
        bind: Option<SocketAddr>,
        /// Endpoint to report to control, e.g. this host's LAN address and the `--bind` port.
        #[arg(long, requires_all = ["identity_file", "hostname"])]
        advertise: Option<SocketAddr>,
        /// The enrolled name, reported with `--advertise`.
        #[arg(long)]
        hostname: Option<String>,
        /// Before load, also wait for the node to ping us (it holds our netmap endpoint). Implied
        /// by `--advertise`; use it with `--bind` when a `hold` process advertises instead.
        #[arg(long)]
        await_node_ping: bool,
    },
    /// Hold one control session that reports `--advertise` as the peer's endpoint, until killed.
    /// Run it for a whole benchmark matrix, so short-lived `peer` runs (with `--bind` and
    /// `--await-node-ping`) don't each reconnect: back-to-back sessions of one node race in
    /// control, and a late "offline" can strip the endpoint from the node mid-run.
    Hold {
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long)]
        disco_key_file: PathBuf,
        #[arg(long)]
        identity_file: PathBuf,
        #[arg(long)]
        advertise: SocketAddr,
        /// The name the peer was enrolled with; control renames a node to whatever a session
        /// reports (the OS hostname when none is given).
        #[arg(long)]
        hostname: String,
    },
    /// Test fixture only: raw WG + production PortGate/forwarder, virtual dst mapped to local sink.
    Fixture {
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long)]
        peer_key: String,
        #[arg(long, requires = "peer_disco_key")]
        disco_key_file: Option<PathBuf>,
        #[arg(long, requires = "disco_key_file")]
        peer_disco_key: Option<String>,
        #[arg(long, default_value = "127.0.0.1:0")]
        bind: SocketAddr,
        #[arg(long, default_value = "100.64.0.1")]
        peer_ip: Ipv4Addr,
        #[arg(long, default_value_t = 256)]
        tcp_buf_kb: usize,
    },
}
fn unhex(value: &str) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    if value.trim().len() != 64 || !value.is_ascii() {
        return Err("key must be 64 hex characters".into());
    }
    let mut bytes = zeroize::Zeroizing::new([0; 32]);
    for (n, out) in bytes.iter_mut().enumerate() {
        *out = u8::from_str_radix(&value.trim()[n * 2..n * 2 + 2], 16)?;
    }
    Ok(bytes)
}
fn private(file: &PathBuf) -> Result<NodePrivateKey> {
    let text = zeroize::Zeroizing::new(std::fs::read_to_string(file)?);
    let bytes = unhex(&text)?;
    Ok((*bytes).into())
}
fn public(text: &str) -> Result<NodePublicKey> {
    Ok((*unhex(text)?).into())
}
fn keygen(file: PathBuf) -> Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let key = NodePrivateKey::random();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut output = options.open(file)?;
    for byte in zeroize::Zeroizing::new(key.to_bytes()).iter() {
        write!(output, "{byte:02x}")?;
    }
    output.write_all(b"\n")?;
    println!(
        "{}",
        key.public_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    Ok(())
}
fn endpoint(key: NodePrivateKey, peer: NodePublicKey) -> Endpoint {
    let mut endpoint = Endpoint::new(key.into());
    endpoint.upsert_peer(
        PeerId(1),
        PeerConfig {
            key: peer,
            psk: [0; 32].into(),
            persistent_keepalive_interval: None,
        },
    );
    endpoint
}

struct Disco {
    private: DiscoPrivateKey,
    our_node: NodePublicKey,
    peer: DiscoPublicKey,
    peer_node: NodePublicKey,
}
fn disco(
    file: Option<PathBuf>,
    peer: Option<String>,
    ours: NodePublicKey,
    theirs: NodePublicKey,
) -> Result<Option<Disco>> {
    file.zip(peer)
        .map(|(file, peer)| {
            let text = zeroize::Zeroizing::new(std::fs::read_to_string(file)?);
            let raw = unhex(&text)?;
            Ok(Disco {
                private: (*raw).into(),
                our_node: ours,
                peer: (*unhex(&peer)?).into(),
                peer_node: theirs,
            })
        })
        .transpose()
}
async fn send(
    socket: &UdpSocket,
    bytes: &[u8],
    address: SocketAddr,
    connected: bool,
) -> Result<()> {
    if connected {
        socket.send(bytes).await?;
    } else {
        socket.send_to(bytes, address).await?;
    }
    Ok(())
}
async fn consume_disco(
    socket: &UdpSocket,
    state: Option<&Disco>,
    packet: &mut [u8],
    from: SocketAddr,
    connected: bool,
) -> Result<bool> {
    if !ts_magicsock::looks_like_disco(packet) {
        return Ok(false);
    }
    if let Some(state) = state
        && let Ok(ts_magicsock::Inbound::Ping {
            sender,
            claimed_node_key,
            tx_id,
        }) = ts_magicsock::open(&state.private, packet)
        && sender == state.peer
        && claimed_node_key == Some(state.peer_node)
    {
        let pong = ts_magicsock::seal_pong(&state.private, &state.peer, tx_id, from)?;
        send(socket, &pong, from, connected).await?;
    }
    Ok(true)
}
async fn enroll(
    key: PathBuf,
    disco_file: PathBuf,
    identity: PathBuf,
    auth: Option<PathBuf>,
    hostname: String,
    target: Option<String>,
) -> Result<()> {
    let node_key = private(&key)?;
    let text = zeroize::Zeroizing::new(std::fs::read_to_string(disco_file)?);
    let raw = unhex(&text)?;
    let disco_key: DiscoPrivateKey = (*raw).into();
    let mut keys: ts_keys::NodeState = if identity.exists() {
        let text = zeroize::Zeroizing::new(std::fs::read_to_string(&identity)?);
        let state: ts_keys::PersistState = serde_json::from_str(&text)?;
        ts_keys::NodeState::from(state)
    } else {
        let mut keys = ts_keys::NodeState::generate();
        keys.node_keys = node_key.clone().into();
        keys.disco_keys = disco_key.clone().into();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        serde_json::to_writer(
            options.open(&identity)?,
            &ts_keys::PersistState::from(&keys),
        )?;
        keys
    };
    keys.disco_keys = disco_key.clone().into();
    if keys.node_keys.public != node_key.public_key()
        || keys.disco_keys.public != disco_key.public_key()
    {
        return Err("identity file does not match supplied keys".into());
    }
    let auth = match auth {
        Some(path) => Some(zeroize::Zeroizing::new(std::fs::read_to_string(path)?)),
        None => None,
    };
    let interactive = auth.is_none();
    let config = ts_control::Config {
        hostname: Some(hostname),
        ephemeral: false,
        ..Default::default()
    };
    let runtime =
        ts_runtime::Runtime::spawn(config, auth.as_ref().map(|a| a.trim().to_owned()), keys)
            .await?;
    // Interactive login: wait_until_running reports NeedsLogin(url) right away; show the URL and
    // keep waiting until it is approved.
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(if interactive { 3600 } else { 90 });
    let mut shown = None;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match runtime.wait_until_running(Some(left)).await {
            Ok(()) => break,
            Err(ts_runtime::RegistrationError::NeedsLogin(url))
                if interactive && !left.is_zero() =>
            {
                if shown.as_ref() != Some(&url) {
                    println!("open this URL to approve the benchmark peer: {url}");
                    shown = Some(url);
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
    let me = runtime
        .status()
        .await?
        .self_node
        .ok_or("control returned no self node")?;
    let target = if let Some(name) = target {
        let tracker = runtime
            .peer_tracker
            .upgrade()
            .ok_or("peer tracker stopped")?;
        let peer = tokio::time::timeout(
            Duration::from_secs(5),
            tracker.ask(ts_runtime::peer_tracker::PeerByName { name }),
        )
        .await??
        .ok_or("target not in netmap")?;
        Some(
            serde_json::json!({"node_key":peer.node_key.as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),
            "node_disco_key":peer.disco_key.map(|key|key.as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>())}),
        )
    } else {
        None
    };
    println!(
        "{}",
        serde_json::json!({"target":target,"source":me.ipv4.to_string(),"node_key":node_key.public_key().as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),
        "disco_key":disco_key.public_key().as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),"hostname":me.display_name})
    );
    drop(runtime);
    Ok(())
}

/// Open a control session for an enrolled identity and report `advertise` as its only endpoint.
/// The returned client keeps the session (and so the peer's netmap endpoint) alive while held.
async fn advertise_endpoint(
    key: &PathBuf,
    disco_file: Option<&PathBuf>,
    identity: &PathBuf,
    advertise: SocketAddr,
    hostname: String,
) -> Result<ts_control::AsyncControlClient> {
    let disco_file = disco_file.ok_or("--advertise needs --disco-key-file")?;
    let node_key = private(key)?;
    let text = zeroize::Zeroizing::new(std::fs::read_to_string(disco_file)?);
    let disco_key: DiscoPrivateKey = (*unhex(&text)?).into();
    let text = zeroize::Zeroizing::new(std::fs::read_to_string(identity)?);
    let mut keys = ts_keys::NodeState::from(serde_json::from_str::<ts_keys::PersistState>(&text)?);
    keys.disco_keys = disco_key.into();
    if keys.node_keys.public != node_key.public_key() {
        return Err("identity file does not match the key file".into());
    }
    let (auth_url, _) = tokio::sync::watch::channel(None);
    let (mut client, stream) = ts_control::AsyncControlClient::connect(
        &ts_control::Config {
            hostname: Some(hostname),
            ..Default::default()
        },
        &keys,
        None,
        auth_url,
    )
    .await?;
    // Drain netmap updates so the session's broadcast never backs up.
    tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut stream = std::pin::pin!(stream);
        while stream.next().await.is_some() {}
    });
    client
        .set_endpoints(vec![ts_control::Endpoint {
            endpoint: advertise,
            ty: ts_control::EndpointType::Local,
        }])
        .await;
    Ok(client)
}

/// One owner for mutable WG endpoint/replay/timers. Drain ready packets in bounded batches.
async fn bridge(
    socket: UdpSocket,
    mut endpoint: Endpoint,
    mut outbound: WakingPipeReceiver,
    admit: impl Fn(&[u8]) -> bool + Send + 'static,
    remote: Option<SocketAddr>,
    source: Option<Ipv4Addr>,
    disco: Option<Disco>,
) -> Result<()> {
    let fixed_remote = remote;
    let mut remote = remote;
    let mut buffer = [0u8; 2048];
    let mut ping = tokio::time::interval(Duration::from_secs(5));
    loop {
        let event = endpoint.next_event().map(|event| event.end());
        let outgoing = tokio::select! {
            received = socket.recv_from(&mut buffer) => {
                let (n, from) = received?;
                if fixed_remote.is_some_and(|address| address != from) { continue; }
                if consume_disco(&socket,disco.as_ref(),&mut buffer[..n],from,fixed_remote.is_some()).await? { continue; }
                let mut batch = vec![PacketMut::from(&buffer[..n])];
                for _ in 1..64 {
                    match socket.try_recv_from(&mut buffer) {
                        Ok((n, address)) if address == from => {
                            if !consume_disco(&socket,disco.as_ref(),&mut buffer[..n],from,fixed_remote.is_some()).await? {
                                batch.push(PacketMut::from(&buffer[..n]));
                            }
                        },
                        Ok(_) => {},
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error) => return Err(error.into()),
                    }
                }
                let received = endpoint.recv(batch);
                if !received.to_peers.is_empty() || !received.to_local.is_empty() { remote = Some(from); }
                for (_, packets) in received.to_local {
                    for packet in packets {
                        let bytes = packet.as_ref();
                        // Empty keepalives aren't IP; reject v6 and forged fixture sources.
                        if bytes.len() >= 20 && bytes[0] >> 4 == 4 {
                            let n = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
                            if n <= bytes.len() && n >= 20 && source.is_none_or(|ip| bytes[12..16] == ip.octets()) {
                                let _accepted = admit(&bytes[..n]);
                            }
                        }
                    }
                }
                received.to_peers
            }
            first = outbound.recv_async() => {
                let Some(first) = first else { return Ok(()); };
                let mut packets = vec![PacketMut::from(first.as_ref())];
                for _ in 1..64 {
                    let Some(more) = outbound.try_recv() else { break; };
                    packets.push(PacketMut::from(more.as_ref()));
                }
                endpoint.send([(PeerId(1), packets)]).to_peers
            }
            _ = ping.tick(), if disco.is_some() && remote.is_some() => {
                let state=disco.as_ref().unwrap();
                let frame=ts_magicsock::seal_ping(&state.private,state.our_node,&state.peer,ts_magicsock::random_tx_id())?;
                send(&socket,&frame,remote.unwrap(),fixed_remote.is_some()).await?;
                continue;
            }
            _ = async {
                match event {
                    Some(due) => tokio::time::sleep_until(due.into()).await,
                    None => std::future::pending().await,
                }
            } => endpoint.dispatch_events(Instant::now()).to_peers,
        };
        if let Some(address) = remote {
            for (_, packets) in outgoing {
                for packet in packets {
                    if fixed_remote.is_some() {
                        socket.send(packet.as_ref()).await?;
                    } else {
                        socket.send_to(packet.as_ref(), address).await?;
                    }
                }
            }
        }
        tokio::task::yield_now().await;
    }
}

fn stack(
    buffer_kb: usize,
) -> (
    Netstack<ts_netstack_smoltcp::WakingPipeDev>,
    ts_netstack_smoltcp::WakingPipe,
) {
    let (mut stack, pipe) = ts_netstack_smoltcp::piped(netcore::Config {
        mtu: 1280,
        tcp_buffer_size: buffer_kb * 1024,
        ..Default::default()
    });
    stack.set_work_budget(Some(128));
    (stack, pipe)
}
struct FixtureDialer(SocketAddr);
impl RealDialer for FixtureDialer {
    async fn dial_tcp(
        &self,
        _: FlowClass,
        _: SocketAddr,
    ) -> std::result::Result<tokio::net::TcpStream, DialError> {
        HostExitDialer.dial_tcp(FlowClass::Subnet, self.0).await
    }
    async fn dial_udp(
        &self,
        _: FlowClass,
        dst: SocketAddr,
    ) -> std::result::Result<DialedUdp, DialError> {
        Err(DialError::ForbiddenDestination(dst))
    }
}
async fn sink(listener: TcpListener) -> Result<()> {
    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let result: std::io::Result<()> = async {
                let mode = socket.read_u8().await?;
                let mut buf = vec![0u8; 64 * 1024];
                if mode == b'D' {
                    let mut remaining = socket.read_u64().await?;
                    while remaining > 0 {
                        let n = remaining.min(buf.len() as u64) as usize;
                        socket.write_all(&buf[..n]).await?;
                        remaining -= n as u64;
                    }
                    socket.shutdown().await?;
                    return Ok(());
                }
                if mode == b'd' {
                    loop {
                        socket.write_all(&buf).await?;
                    }
                }
                loop {
                    let n = socket.read(&mut buf).await?;
                    if n == 0 {
                        return Ok(());
                    }
                    if mode == b'e' {
                        socket.write_all(&buf[..n]).await?;
                    }
                }
            }
            .await;
            drop(result);
        });
    }
}
async fn fixture(
    key: PathBuf,
    disco_file: Option<PathBuf>,
    peer_disco: Option<String>,
    peer: String,
    bind: SocketAddr,
    peer_ip: Ipv4Addr,
    buffer: usize,
) -> Result<()> {
    if !bind.ip().is_loopback() {
        return Err(
            "fixture is restricted to loopback; use peer mode for an admitted live peer".into(),
        );
    }
    let socket = UdpSocket::bind(bind).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let sink_address = listener.local_addr()?;
    let (mut stack, pipe) = stack(buffer);
    let channel = stack.command_channel();
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        stack.run_tokio().await;
        Ok(())
    });
    channel.set_any_ip(true).await?;
    let (forwarder, gate) = Forwarder::all_ports_gated(
        channel,
        RouteTable::new(["0.0.0.0/0".parse()?]),
        FixtureDialer(sink_address),
        pipe.tx,
    );
    tasks.spawn(async move { forwarder.run().await.map_err(Into::into) });
    tasks.spawn(sink(listener));
    let key = private(&key)?;
    let peer = public(&peer)?;
    let disco = disco(disco_file, peer_disco, key.public_key(), peer)?;
    let ep = endpoint(key, peer);
    println!(
        "{}",
        serde_json::json!({"fixture_endpoint":socket.local_addr()?.to_string(),"pid":std::process::id(),"virtual_destination":"203.0.113.1:9000"})
    );
    tasks.spawn(bridge(
        socket,
        ep,
        pipe.rx,
        move |p| gate.ingress(p) != ts_forwarder::Admit::Dropped,
        None,
        Some(peer_ip),
        disco,
    ));
    tokio::select! {
        done = tasks.join_next() => { done.ok_or("no fixture tasks")???; Err("fixture task exited".into()) }
        result = tokio::signal::ctrl_c() => { result?; Ok(()) }
    }
}
#[allow(clippy::too_many_arguments)]
async fn peer(
    key: PathBuf,
    node: String,
    disco_file: Option<PathBuf>,
    node_disco: Option<String>,
    remote: SocketAddr,
    source: Ipv4Addr,
    dst: SocketAddr,
    flows: usize,
    bytes: u64,
    direction: String,
    timeout: u64,
    buffer: usize,
    wait_for_start: bool,
    legacy_stream: bool,
    bind: Option<SocketAddr>,
    await_node_ping: bool,
) -> Result<()> {
    if !dst.is_ipv4() || flows == 0 || bytes == 0 || flows > 16000 {
        return Err("IPv4 destination, positive bytes and 1..16000 flows required".into());
    }
    let key = private(&key)?;
    let node = public(&node)?;
    let disco = disco(disco_file, node_disco, key.public_key(), node)?;
    let ep = endpoint(key, node);
    let udp = match bind {
        Some(bind) => std::net::UdpSocket::bind(bind)?,
        None => std::net::UdpSocket::bind(if remote.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?,
    };
    let options = socket2::SockRef::from(&udp);
    options.set_recv_buffer_size(7 * 1024 * 1024)?;
    options.set_send_buffer_size(7 * 1024 * 1024)?;
    udp.connect(remote)?;
    udp.set_nonblocking(true)?;
    let socket = UdpSocket::from_std(udp)?;
    if let Some(state) = disco.as_ref() {
        let tx = ts_magicsock::random_tx_id();
        let ping = ts_magicsock::seal_ping(&state.private, state.our_node, &state.peer, tx)?;
        // With an advertised endpoint, also wait for the node to ping us: that only happens once
        // the node holds our netmap endpoint, which is what lets it attribute our WireGuard data.
        let (mut ponged, mut pinged) = (false, !await_node_ping);
        tokio::time::timeout(Duration::from_secs(if await_node_ping { 60 } else { 10 }),async {
            let mut retry=tokio::time::interval(Duration::from_millis(500));
            let mut buffer=[0u8;2048];
            loop {
                if ponged && pinged { return Ok::<_,Box<dyn Error+Send+Sync>>(()); }
                tokio::select! {
                    _=retry.tick() => { socket.send(&ping).await?; }
                    result=socket.recv_from(&mut buffer) => {
                        let (n,from)=result?;
                        let packet=&mut buffer[..n];
                        match ts_magicsock::open(&state.private,packet) {
                            Ok(ts_magicsock::Inbound::Pong{sender,tx_id,..}) if sender==state.peer && tx_id==tx => {
                                ponged = true;
                            }
                            Ok(ts_magicsock::Inbound::Ping{sender,claimed_node_key,tx_id}) if sender==state.peer && claimed_node_key==Some(state.peer_node) => {
                                let pong=ts_magicsock::seal_pong(&state.private,&state.peer,tx_id,from)?;
                                socket.send(&pong).await?;
                                pinged = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }).await.map_err(|_| "disco preflight timed out: check control admission and node disco key")??;
    }
    let (mut stack, pipe) = stack(buffer);
    let channel = stack.command_channel();
    let mut drivers: JoinSet<Result<()>> = JoinSet::new();
    drivers.spawn(async move {
        stack.run_tokio().await;
        Ok(())
    });
    channel.set_ips([source.into()]).await?;
    let tx: WakingPipeSender = pipe.tx;
    drivers.spawn(bridge(
        socket,
        ep,
        pipe.rx,
        move |p| tx.try_send(p),
        Some(remote),
        None,
        disco,
    ));
    let start = Instant::now();
    let mut jobs = JoinSet::new();
    let go = Arc::new(tokio::sync::Barrier::new(flows));
    let base_port: u16 = rand::random_range(20000..(u16::MAX - flows as u16));
    let load_start = Arc::new(std::sync::OnceLock::new());
    let (external_start, _) = tokio::sync::watch::channel(false);
    for n in 0..flows {
        let channel = channel.clone();
        let go = go.clone();
        let load_start = load_start.clone();
        let down = direction == "down";
        let external_start = external_start.clone();
        let mut started = external_start.subscribe();
        let progress = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let seen = progress.clone();
        jobs.spawn(async move {
            let run = async {
                let local = SocketAddr::new(source.into(), base_port + n as u16);
                let mut stream = channel.tcp_connect(local, dst).await?;
                let leader = go.wait().await;
                if wait_for_start {
                    if leader.is_leader() {
                        println!(
                            "{}",
                            serde_json::json!({"ready":true,"pid":std::process::id()})
                        );
                        let input = tokio::task::spawn_blocking(|| {
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input)?;
                            Ok::<_, std::io::Error>(input)
                        })
                        .await??;
                        if input.trim() != "GO" {
                            return Err("expected GO on stdin".into());
                        }
                        external_start.send_replace(true);
                    }
                    drop(started.wait_for(|started| *started).await?);
                }
                load_start.get_or_init(|| (Instant::now(), process_cpu()));
                let started = Instant::now();
                let mode = if down {
                    if legacy_stream { b'd' } else { b'D' }
                } else {
                    b'u'
                };
                stream.write_u8(mode).await?;
                if down && !legacy_stream {
                    stream.write_u64(bytes).await?;
                }
                let mut buf = vec![0; 64 * 1024];
                let mut moved = 0u64;
                while moved < bytes {
                    let size = (bytes - moved).min(buf.len() as u64) as usize;
                    let amount = if down {
                        let got = stream.read(&mut buf[..size]).await?;
                        if got == 0 {
                            return Err("short download".into());
                        }
                        got
                    } else {
                        stream.write_all(&buf[..size]).await?;
                        size
                    };
                    moved += amount as u64;
                    progress.store(moved, std::sync::atomic::Ordering::Relaxed);
                }
                if !down {
                    stream.shutdown().await?;
                    let mut ack = [0];
                    if stream.read(&mut ack).await? != 0 {
                        return Err("unexpected sink acknowledgement".into());
                    }
                }
                let report = serde_json::json!({
                    "flow": n, "bytes": moved, "seconds": started.elapsed().as_secs_f64(),
                    "start_s": started.duration_since(start).as_secs_f64(),
                    "completion_s": start.elapsed().as_secs_f64()
                });
                Ok::<_, Box<dyn Error + Send + Sync>>((report, stream))
            };
            tokio::time::timeout(Duration::from_secs(timeout), run)
                .await
                .map_err(|_| {
                    format!(
                        "flow {n} (local port {}) timed out after {} of {bytes} bytes",
                        base_port + n as u16,
                        seen.load(std::sync::atomic::Ordering::Relaxed)
                    )
                })?
        });
    }

    let mut rows = Vec::new();
    let mut streams = Vec::new();
    while !jobs.is_empty() {
        tokio::select! {
            result=jobs.join_next() => { let (row,stream)=result.ok_or("missing flow")???; rows.push(row); streams.push(stream); },
            failed=drivers.join_next() => { failed.ok_or("missing driver")???; return Err("packet driver stopped".into()); }
        }
    }
    let wall = start.elapsed().as_secs_f64();
    let generator_cpu_s = process_cpu()
        .zip(load_start.get().and_then(|(_, cpu)| *cpu))
        .map(|(end, begin)| end - begin);
    let window_start = rows
        .iter()
        .map(|r| r["start_s"].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min);
    let window_end = rows
        .iter()
        .map(|r| r["completion_s"].as_f64().unwrap())
        .fold(0.0, f64::max);
    let transfer_seconds = window_end - window_start;
    println!(
        "{}",
        serde_json::json!({"pid":std::process::id(),"direction":direction,"flows":rows,"generator_cpu_s":generator_cpu_s,
        "bytes":bytes*flows as u64,"seconds_including_setup":wall,"transfer_seconds":transfer_seconds,"mbit_s":bytes as f64*flows as f64*8.0/transfer_seconds/1e6,"mbit_s_including_setup":bytes as f64*flows as f64*8.0/wall/1e6})
    );
    if wait_for_start {
        // The orchestrator sends STOP only after every peer has reported, and peers can finish far
        // apart (uneven upload completion), so wait as long as the transfer itself may take.
        let input = tokio::time::timeout(
            Duration::from_secs(timeout),
            tokio::task::spawn_blocking(|| {
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                Ok::<_, std::io::Error>(input)
            }),
        )
        .await???;
        if input.trim() != "STOP" {
            return Err("expected STOP on stdin".into());
        }
    }
    // Teardown is best-effort: the transfer was already measured and reported. A connection that
    // finished long before STOP (peers can complete far apart) may already be fully closed, and
    // reading it then reports an invalid socket state rather than EOF. Report such problems on
    // stderr without failing the run.
    if !legacy_stream {
        for mut stream in streams {
            jobs.spawn(async move {
                if let Err(e) = stream.shutdown().await {
                    eprintln!("teardown: shutdown: {e}");
                }
                let mut buf = [0u8; 1024];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("teardown: read after shutdown: {e}");
                            break;
                        }
                    }
                }
                Ok::<_, Box<dyn Error + Send + Sync>>((serde_json::Value::Null, stream))
            });
        }
        let drained = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(closed) = jobs.join_next().await {
                if let Err(e) = closed {
                    eprintln!("teardown: close task failed: {e}");
                }
            }
        })
        .await;
        if drained.is_err() {
            eprintln!("teardown: connections did not close within 5 s");
        }
    }
    // Flush the last FIN/ACK after the measured window; never strand remote sockets by immediately
    // dropping the crypto/UDP driver after a fire-and-forget netstack close.
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}
#[cfg(unix)]
fn process_cpu() -> Option<f64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid writable timespec and a process CPU clock, with no borrowed resources.
    if unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) } == 0 {
        Some(time.tv_sec as f64 + time.tv_nsec as f64 / 1e9)
    } else {
        None
    }
}
#[cfg(not(unix))]
fn process_cpu() -> Option<f64> {
    None
}

fn main() -> Result<()> {
    let args = Args::parse();
    // Off unless RUST_LOG is set; stdout carries the JSON protocol, so logs go to stderr.
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }
    if let Command::Keygen { file } = args.command {
        return keygen(file);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.workers.max(1))
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        match args.command {
            Command::Keygen { .. } => unreachable!(),
            Command::Enroll {
                key_file,
                disco_key_file,
                identity_file,
                auth_key_file,
                hostname,
                target,
            } => {
                enroll(
                    key_file,
                    disco_key_file,
                    identity_file,
                    auth_key_file,
                    hostname,
                    target,
                )
                .await
            }
            Command::Hold {
                key_file,
                disco_key_file,
                identity_file,
                advertise,
                hostname,
            } => {
                let _control = advertise_endpoint(
                    &key_file,
                    Some(&disco_key_file),
                    &identity_file,
                    advertise,
                    hostname,
                )
                .await?;
                println!("{}", serde_json::json!({"holding": advertise.to_string()}));
                std::future::pending::<()>().await;
                Ok(())
            }
            Command::Fixture {
                key_file,
                peer_key,
                bind,
                peer_ip,
                tcp_buf_kb,
                disco_key_file,
                peer_disco_key,
            } => {
                fixture(
                    key_file,
                    disco_key_file,
                    peer_disco_key,
                    peer_key,
                    bind,
                    peer_ip,
                    tcp_buf_kb,
                )
                .await
            }
            Command::Peer {
                key_file,
                node_key,
                endpoint,
                source,
                destination,
                flows,
                bytes,
                direction,
                timeout,
                tcp_buf_kb,
                disco_key_file,
                node_disco_key,
                wait_for_start,
                legacy_stream,
                identity_file,
                bind,
                advertise,
                await_node_ping,
                hostname,
            } => {
                let _control = match (identity_file, advertise, hostname) {
                    (Some(identity), Some(advertise), Some(hostname)) => Some(
                        advertise_endpoint(
                            &key_file,
                            disco_key_file.as_ref(),
                            &identity,
                            advertise,
                            hostname,
                        )
                        .await?,
                    ),
                    _ => None,
                };
                peer(
                    key_file,
                    node_key,
                    disco_key_file,
                    node_disco_key,
                    endpoint,
                    source,
                    destination,
                    flows,
                    bytes,
                    direction,
                    timeout,
                    tcp_buf_kb,
                    wait_for_start,
                    legacy_stream,
                    bind,
                    await_node_ping || _control.is_some(),
                )
                .await
            }
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    result
}
