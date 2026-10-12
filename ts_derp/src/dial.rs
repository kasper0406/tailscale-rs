//! Logic for connecting to a derp server.

use core::net::{IpAddr, SocketAddr};
use std::io;

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    task::JoinSet,
};
use ts_tls_util::ServerName;

use crate::{IpUsage, ServerConnInfo, TlsValidationConfig};

/// Cancellable asynchronous hostname lookup. TCP/TLS socket creation stays on the dialing runtime.
pub type Resolver = std::sync::Arc<
    dyn Fn(
            String,
            u16,
        ) -> core::pin::Pin<
            Box<dyn core::future::Future<Output = io::Result<Vec<SocketAddr>>> + Send>,
        > + Send
        + Sync,
>;

/// Run system hostname resolution on the caller runtime, not a packet runtime's blocking pool.
/// Cancelling a dial aborts the caller's async waiter. OS getaddrinfo itself is not cancellable;
/// caller-runtime owners must use bounded shutdown if the entire process must exit on DNS failure.
pub fn resolver_on(runtime: tokio::runtime::Handle) -> Resolver {
    struct Lookup(tokio::task::JoinHandle<io::Result<Vec<SocketAddr>>>);
    impl Drop for Lookup {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    std::sync::Arc::new(move |hostname, port| {
        let mut lookup = Lookup(runtime.spawn(async move {
            tokio::net::lookup_host((hostname, port))
                .await
                .map(|addrs| addrs.collect())
        }));
        Box::pin(async move { (&mut lookup.0).await.map_err(io::Error::other)? })
    })
}

/// Error that may occur while dialing a derp server.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// IO error occurred.
    #[error("io error occurred")]
    Io,

    /// Bad parameter.
    #[error("invalid parameter")]
    InvalidParam,
}

impl From<io::Error> for Error {
    fn from(_: io::Error) -> Self {
        Error::Io
    }
}

/// Dial a TLS connection to the first server that can successfully establish a TCP
/// connection (they are tried serially in the order presented).
///
/// A returned error is specifically an error constructing the TLS connection.
///
/// Returns `Ok(None)` iff no server could be reached, either due to connectivity errors or
/// because they were not configured to be reachable (see the note in [`dial_region_tcp`]).
/// Self-signed server certs are supported via SHA-256 cert pinning (see
/// [`TlsValidationConfig::SelfSigned`]).
pub async fn dial_region_tls<'c>(
    servers: impl IntoIterator<Item = &'c ServerConnInfo>,
) -> Result<
    Option<(
        impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
        &'c ServerConnInfo,
        SocketAddr,
    )>,
    Error,
> {
    dial_region_tls_with_resolver(servers, None).await
}

/// Dial TLS with a supplied hostname resolver.
pub async fn dial_region_tls_with_resolver<'c>(
    servers: impl IntoIterator<Item = &'c ServerConnInfo>,
    resolver: Option<Resolver>,
) -> Result<
    Option<(
        impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
        &'c ServerConnInfo,
        SocketAddr,
    )>,
    Error,
> {
    let Some((conn, server)) = dial_region_tcp_with_resolver(servers, resolver).await else {
        return Ok(None);
    };
    let remote_addr = conn.peer_addr()?;

    let tls_conn = match &server.tls_validation_config {
        TlsValidationConfig::CommonName { common_name } => {
            ts_tls_util::connect(
                ServerName::try_from(common_name.clone()).map_err(|e| {
                    tracing::error!(error = %e, "derp common name");
                    Error::InvalidParam
                })?,
                conn,
            )
            .await?
        }
        #[cfg(feature = "insecure-for-tests")]
        TlsValidationConfig::InsecureForTests => {
            tracing::warn!(%server.hostname, "using insecure TLS for tests");

            ts_tls_util::connect_insecure(
                ServerName::try_from(server.hostname.clone()).map_err(|e| {
                    tracing::error!(error = %e, "derp hostname");
                    Error::InvalidParam
                })?,
                conn,
            )
            .await?
        }
        TlsValidationConfig::SelfSigned { sha256 } => {
            // Self-hosted / self-signed DERP (e.g. embedded in a self-hosted control plane): accept
            // exactly the cert whose DER matches the configured SHA-256 pin. The
            // SNI still uses the configured hostname (typically an IP literal),
            // but trust is the pin, not the public PKI. Fail-closed: a mismatched
            // cert is a hard handshake error, never a downgrade.
            ts_tls_util::connect_pinned(
                ServerName::try_from(server.hostname.clone())
                    .map_err(|e| {
                        tracing::error!(error = %e, "derp self-signed hostname");
                        Error::InvalidParam
                    })?
                    .to_owned(),
                *sha256,
                conn,
            )
            .await?
        }
    };

    Ok(Some((tls_conn, server, remote_addr)))
}

/// Attempt to establish a TCP connection to one of the listed servers.
///
/// The servers are attempted serially; both IPv4 and IPv6 connections are concurrently
/// attempted for each server (if respectively enabled in the [`ServerConnInfo`]).
///
/// Returns `None` if no server could be dialed, whether due to encountered errors or
/// because they were not configured to be reachable (both ipv4/ipv6 disabled or stun_only).
pub async fn dial_region_tcp<'c>(
    servers: impl IntoIterator<Item = &'c ServerConnInfo>,
) -> Option<(TcpStream, &'c ServerConnInfo)> {
    dial_region_tcp_with_resolver(servers, None).await
}

/// Dial TCP with DNS isolated from socket registration.
pub async fn dial_region_tcp_with_resolver<'c>(
    servers: impl IntoIterator<Item = &'c ServerConnInfo>,
    resolver: Option<Resolver>,
) -> Option<(TcpStream, &'c ServerConnInfo)> {
    for server in servers {
        if server.stun_only {
            tracing::trace!(%server.hostname, "server is stun only, skip");
            continue;
        }

        // SelfSigned is now supported via SHA-256 cert pinning in dial_region_tls.
        // InsecureForTests is allowed through -- TLS verification is skipped in dial_region_tls.

        match dial_server_with_resolver(server, resolver.clone()).await {
            Ok(Some(conn)) => {
                tracing::trace!(
                    remote_addr = %conn.peer_addr().unwrap_or((core::net::Ipv4Addr::UNSPECIFIED, 0).into()),
                    %server.hostname,
                    "derp tcp dial ok",
                );
                return Some((conn, server));
            }
            Ok(None) => {
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, %server.hostname, "failed tcp dialing server");
                continue;
            }
        }
    }

    None
}

/// Attempt a TCP connection to a single server.
///
/// IPv4 and IPv6 connections are raced concurrently: the first one to establish wins and is
/// used.
///
/// Returns `None` iff the server has both IPv4 and IPv6 configured as [`IpUsage::Disable`].
pub async fn dial_server(server: &ServerConnInfo) -> Result<Option<TcpStream>, Error> {
    dial_server_with_resolver(server, None).await
}

/// Dial one server with an optional caller-runtime resolver.
pub async fn dial_server_with_resolver(
    server: &ServerConnInfo,
    resolver: Option<Resolver>,
) -> Result<Option<TcpStream>, Error> {
    let mut js = JoinSet::new();

    // TODO(npry): respect ipv6 preference. The go client just adds a flat 200ms sleep to
    //  the ipv4 sample in this case.

    js.spawn(dial_by_ipusage(
        server.ipv4,
        server.hostname.clone(),
        server.https_port,
        resolver.clone(),
    ));
    js.spawn(dial_by_ipusage(
        server.ipv6,
        server.hostname.clone(),
        server.https_port,
        resolver,
    ));

    let mut last_error = None;

    while let Some(task) = js.join_next().await {
        // A JoinError is either a cancellation (not the case by inspection) or a task panic.
        // Hence, this unwrap just forwards the inner panic, which is the behavior we want here.
        match task.unwrap() {
            Ok(Some(stream)) => return Ok(Some(stream)),
            Ok(None) => {
                continue;
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }

    if let Some(e) = last_error {
        Err(e.into())
    } else {
        Ok(None)
    }
}

#[tracing::instrument(skip_all, level = "trace")]
async fn dial_by_ipusage(
    ip: IpUsage<impl Into<IpAddr>>,
    hostname: String,
    port: u16,
    resolver: Option<Resolver>,
) -> io::Result<Option<TcpStream>> {
    match ip {
        IpUsage::Disable => Ok(None),
        IpUsage::FixedAddr(ip) => {
            let ip = ip.into();

            TcpStream::connect((ip, port)).await.map(Some)
        }
        IpUsage::UseDns => match resolver {
            Some(resolve) => {
                let addrs = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    resolve(hostname, port),
                )
                .await
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "DERP DNS lookup timed out")
                })??;
                TcpStream::connect(addrs.as_slice()).await.map(Some)
            }
            None => TcpStream::connect((hostname, port)).await.map(Some),
        },
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };

    #[tokio::test]
    async fn caller_dns_and_packet_socket_have_independent_lifetimes() {
        let caller = tokio::runtime::Handle::current();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        let (ready, started) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("test-packet-dial".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    let resolver = resolver_on(caller);
                    let socket = dial_by_ipusage(
                        IpUsage::<std::net::Ipv4Addr>::UseDns,
                        "localhost".into(),
                        address.port(),
                        Some(resolver),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    ready.send(()).unwrap();
                    let _ = stopped.await;
                    drop(socket);
                });
            })
            .unwrap();
        let accepted = tokio::time::timeout(Duration::from_secs(2), server.accept())
            .await
            .unwrap()
            .unwrap();
        // Reader readiness on the packet runtime continues independently of the DNS waiter.
        tokio::task::spawn_blocking(move || started.recv_timeout(Duration::from_secs(1)).unwrap())
            .await
            .unwrap();
        drop(accepted);
        stop.send(()).unwrap();
        tokio::task::spawn_blocking(move || thread.join().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stalled_caller_resolver_does_not_hold_packet_runtime_shutdown() {
        let (release, gate) = mpsc::channel();
        let gate = Arc::new(std::sync::Mutex::new(Some(gate)));
        let (entered, started) = tokio::sync::oneshot::channel();
        let entered = Arc::new(std::sync::Mutex::new(Some(entered)));
        let caller = tokio::runtime::Handle::current();
        let resolver: Resolver = Arc::new(move |_, _| {
            let gate = gate.clone();
            let entered = entered.clone();
            let task = caller.spawn_blocking(move || {
                if let Some(tx) = entered.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                gate.lock().unwrap().take().unwrap().recv().unwrap();
                Vec::new()
            });
            Box::pin(async move { task.await.map_err(io::Error::other) })
        });
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (joined, done) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::spawn(async move {
                    drop(
                        dial_by_ipusage(
                            IpUsage::<std::net::Ipv4Addr>::UseDns,
                            "stalled.invalid".into(),
                            443,
                            Some(resolver),
                        )
                        .await,
                    );
                });
                let _ = stopped.await;
            });
            drop(runtime);
            let _ = joined.send(());
        });
        started.await.unwrap();
        stop.send(()).unwrap();
        let bounded = tokio::time::timeout(Duration::from_millis(500), done).await;
        // Release even when the assertion fails: tests must not strand Tokio's blocking pool.
        release.send(()).unwrap();
        tokio::task::spawn_blocking(move || thread.join().unwrap())
            .await
            .unwrap();
        bounded.unwrap().unwrap();
    }
}
