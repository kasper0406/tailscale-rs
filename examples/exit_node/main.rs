//! Run this process as a userspace **exit node** for a tailnet.
//!
//! A minimal, purpose-built runner for experimenting with a residential exit node: it joins the
//! tailnet with a persisted identity, advertises `0.0.0.0/0` (never `::/0`), forwards every TCP/UDP
//! port through the userspace netstack and egresses via this host's own sockets. It does not
//! touch the host's routes, DNS or packet filter, and it does not need root.
//!
//! The auth key is read from a file (never argv or logs) and is only needed for the first
//! registration; afterwards the identity in `--state-dir` is reused.
//!
//! ```sh
//! TS_RS_EXPERIMENT=this_is_unstable_software \
//!   cargo run --release --example exit_node -- \
//!     --state-dir ./state/rust-exit --auth-key-file ~/.config/ts-exit/authkey
//! ```
//!
//! The advertised route still has to be approved in the admin console (or by an `autoApprovers`
//! policy) before peers can select this node.

use std::{error::Error, path::PathBuf, time::Duration};

use clap::Parser;
use tailscale::{Config, Device, config::BadFormatBehavior};
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::filter::LevelFilter;

#[derive(clap::Parser)]
#[command(version, about)]
struct Args {
    /// Directory holding this node's persisted identity (`keys.json`). Created if missing.
    #[arg(long)]
    state_dir: PathBuf,

    /// File containing the auth key. Only read if the identity is not yet registered.
    #[arg(long)]
    auth_key_file: Option<PathBuf>,

    /// Hostname to request.
    #[arg(long, default_value = "rust-exit")]
    hostname: String,

    /// Tags to request (must match the auth key's tags), e.g. `tag:rust-exit`.
    #[arg(long = "tag")]
    tags: Vec<String>,

    /// UDP port for WireGuard + disco. Defaults away from the official client's 41641.
    #[arg(long, default_value_t = 41642)]
    port: u16,

    /// Don't advertise the exit-node route; join the tailnet only (tunnel-only tests).
    #[arg(long)]
    no_advertise: bool,

    /// Per-direction userspace TCP buffer in bytes (netstack default: 256 KiB).
    #[arg(long)]
    tcp_buffer_size: Option<usize>,

    /// Seconds between status lines (peers, direct vs DERP path). 0 disables.
    #[arg(long, default_value_t = 30)]
    status_interval: u64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let args = Args::parse();

    tokio::fs::create_dir_all(&args.state_dir).await?;
    let key_file = args.state_dir.join("keys.json");
    let first_run = !tokio::fs::try_exists(&key_file).await?;

    let auth_key = match (&args.auth_key_file, first_run) {
        (Some(p), _) => Some(tokio::fs::read_to_string(p).await?.trim().to_owned()),
        (None, true) => return Err("first run needs --auth-key-file".into()),
        (None, false) => None,
    };

    let config = Config {
        key_state: tailscale::config::load_key_file(&key_file, BadFormatBehavior::Error).await?,
        client_name: Some("ts-exit-node".to_owned()),
        requested_hostname: Some(args.hostname.clone()),
        requested_tags: args.tags.clone(),
        // A persistent exit node must survive brief disconnects without being GC'd by control.
        ephemeral: false,
        // Userspace only: never take over the host resolver.
        accept_dns: false,
        advertise_exit_node: !args.no_advertise,
        forward_all_ports: !args.no_advertise,
        forward_exit_egress: !args.no_advertise,
        // Egress to the exit node's LAN stays off, like Go's default for exit-node users.
        exit_node_allow_lan_access: false,
        tcp_buffer_size: args.tcp_buffer_size,
        wireguard_listen_port: Some(args.port),
        netmap_cache_dir: None,
        ..Config::default()
    };

    let dev = Device::new(&config, auth_key).await?;
    dev.wait_until_running(Some(Duration::from_secs(90)))
        .await?;
    let (v4, v6) = dev.tailscale_ips().await?;
    tracing::info!(
        ?v4,
        ?v6,
        advertise_exit_node = !args.no_advertise,
        "running"
    );

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut ticker = (args.status_interval > 0)
        .then(|| tokio::time::interval(Duration::from_secs(args.status_interval)));

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = sigterm.recv() => break,
            _ = async { ticker.as_mut().unwrap().tick().await }, if ticker.is_some() => {
                log_status(&dev).await;
            }
        }
    }

    tracing::info!("shutting down");
    let clean = dev.shutdown(Some(Duration::from_secs(5))).await;
    tracing::info!(clean, "stopped");
    Ok(())
}

async fn log_status(dev: &Device) {
    let status = match dev.status().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "status");
            return;
        }
    };
    if let Some(me) = &status.self_node {
        tracing::info!(
            routes = ?me.allowed_routes,
            exit_node = me.is_exit_node,
            health = ?status.health,
            "self"
        );
    }
    for p in status.peers.iter().filter(|p| p.online == Some(true)) {
        let path = match (&p.cur_addr, &p.relay) {
            (Some(a), _) => format!("direct {a}"),
            (None, Some(r)) => format!("derp {r}"),
            (None, None) => "unknown".to_owned(),
        };
        tracing::info!(peer = %p.display_name, ip = %p.ipv4, %path, "peer");
    }
}
