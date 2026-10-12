//! Opt-in placement of the serial packet path on one OS thread.
//!
//! Only the owner stops/joins the runtime; actors hold handles, never the owner. This avoids a
//! reference cycle through tasks running on the runtime. Existing channels remain the boundary.

use std::{future::Future, thread};

use tokio::{runtime::Handle, sync::oneshot, task::JoinSet};

pub(crate) const THREAD_NAME: &str = "ts-packet-pipeline";

pub(crate) struct PipelineThread {
    handle: Handle,
    caller: Handle,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PipelineThread {
    pub(crate) fn start() -> std::io::Result<Self> {
        Self::named(THREAD_NAME)
    }

    pub(crate) fn named(name: &str) -> std::io::Result<Self> {
        let caller = Handle::current();
        let (stop, stopped) = oneshot::channel();
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let name = name.to_owned();
        let thread = thread::Builder::new().name(name.clone()).spawn(move || {
            // Build and drop on the OS thread, including startup failures. A failed OS-thread
            // spawn must not drop a Tokio runtime inside the caller's async context.
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => {
                    drop(ready.send(Ok(runtime.handle().clone())));
                    runtime.block_on(async {
                        crate::pipeline_metrics::start_cpu_sampler(&name);
                        let _ = stopped.await;
                    });
                    // Cancels remaining tasks without waiting for caller actors to reply.
                    drop(runtime);
                }
                Err(error) => {
                    drop(ready.send(Err(error)));
                }
            }
        })?;
        // Only a short startup handshake; no packet task or caller-runtime work is awaited.
        let handle = match started.recv() {
            Ok(Ok(handle)) => handle,
            result => {
                drop(thread.join());
                return Err(match result {
                    Ok(Err(error)) => error,
                    Err(error) => std::io::Error::other(error),
                    Ok(Ok(_)) => unreachable!(),
                });
            }
        };
        Ok(Self {
            handle,
            caller,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    pub(crate) fn handle(&self) -> Handle {
        self.handle.clone()
    }
}

impl Drop for PipelineThread {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            if thread.thread().id() == thread::current().id() {
                // An inline user capture hook may drop the last Runtime Arc here. We cannot
                // join ourselves; let the caller's blocking pool join after this poll returns.
                // Never use this runtime's pool: runtime Drop would wait for its own joiner.
                drop(self.caller.spawn_blocking(move || join(thread)));
            } else {
                join(thread);
            }
        }
    }
}

fn join(thread: thread::JoinHandle<()>) {
    if thread.join().is_err() {
        tracing::error!("packet pipeline thread panicked");
    }
}

/// Owners are separate from the handles cloned into actors. Off keeps ambient placement.
pub(crate) struct PipelineRuntimes {
    packet: PipelineThread,
    supervisor: PipelineThread,
}

impl PipelineRuntimes {
    pub(crate) fn from_env() -> std::io::Result<Option<Self>> {
        let mode = std::env::var("TS_EXPERIMENT_PIPELINE_THREAD").unwrap_or_default();
        if mode != "1" {
            return Ok(None);
        }
        Ok(Some(Self {
            packet: PipelineThread::start()?,
            supervisor: PipelineThread::named("ts-security-guard")?,
        }))
    }
    pub(crate) fn handle(&self) -> Handle {
        self.packet.handle()
    }
    pub(crate) fn supervisor_handle(&self) -> Handle {
        self.supervisor.handle()
    }
}

/// Enter only around synchronous task/socket construction, never across an await.
pub(crate) fn enter<T>(handle: Option<&Handle>, f: impl FnOnce() -> T) -> T {
    let _entered = handle.map(Handle::enter);
    f()
}

pub(crate) fn spawn_on<F>(tasks: &mut JoinSet<F::Output>, handle: Option<&Handle>, future: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    if let Some(handle) = handle {
        tasks.spawn_on(future, handle);
    } else {
        tasks.spawn(future);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    use tokio::sync::{Notify, mpsc};

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn packet_thread_progresses_while_caller_worker_is_cpu_bound() {
        let pipeline = PipelineThread::start().unwrap();
        let mut tasks = JoinSet::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let go = Arc::new(Notify::new());
        let packet_go = go.clone();
        spawn_on(&mut tasks, Some(&pipeline.handle()), async move {
            packet_go.notified().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            tx.send((thread::current().name().unwrap().to_owned(), Instant::now()))
                .unwrap();
        });
        let start = Instant::now();
        let hog = tokio::spawn(async move {
            go.notify_one();
            while start.elapsed() < Duration::from_millis(500) {
                std::hint::spin_loop();
            }
        });
        let (name, progressed) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(name, THREAD_NAME);
        assert!(progressed.duration_since(start) < Duration::from_millis(250));
        hog.await.unwrap();
        tasks.join_next().await.unwrap().unwrap();
    }

    /// Drive the production actor-owned netstack loops and both directions of their pumps.
    /// Listener setup stays on the caller; subsequent packet progress must survive its busy worker.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn application_and_forwarder_packet_pumps_survive_caller_starvation() {
        use kameo::actor::Spawn;
        use netstack::{CreateSocket, netcore::NetstackControl};

        use crate::{
            Env,
            dataplane::{DatapathActivity, OverlayToDataplane},
            env::ForwarderConfig,
            forwarder_actor::ForwarderActor,
            netstack_actor::{GetChannel, NetstackActor},
        };

        tokio::time::timeout(Duration::from_secs(5), async {
            let pipeline = PipelineThread::start().unwrap();
            let (_shutdown, rx) = tokio::sync::watch::channel(false);
            let mut env = Env::new(
                ts_keys::NodeState::generate(),
                rx,
                ForwarderConfig::from_control_config(&ts_control::Config::default()),
            );
            env.packet_runtime = Some(pipeline.handle());
            env.caller_runtime = Some(Handle::current());
            let (app_up, mut app_out) = OverlayToDataplane::for_test(DatapathActivity::new());
            let (app_in, app_down) = mpsc::unbounded_channel();
            let app = env.on_packet_runtime(|| {
                NetstackActor::spawn((env.clone(), Default::default(), app_up, app_down))
            });
            let (app_channel,) = app.ask(GetChannel).await.unwrap();
            app_channel
                .set_ips(["100.64.0.1".parse().unwrap()])
                .await
                .unwrap();
            let socket = app_channel
                .udp_bind("100.64.0.1:40000".parse().unwrap())
                .await
                .unwrap();
            let app_raw = app_channel
                .raw_open(true, netstack::netcore::smoltcp::wire::IpProtocol::Icmp)
                .await
                .unwrap();
            let (fwd_up, mut fwd_out) = OverlayToDataplane::for_test(DatapathActivity::new());
            let (fwd_in, fwd_down) = mpsc::unbounded_channel();
            // Intentionally ambient: all forwarder listeners/splices must stay here.
            let fwd = ForwarderActor::spawn((env.clone(), Default::default(), fwd_up, fwd_down));
            fwd.ask(crate::forwarder_actor::GetChannel).await.unwrap();
            let mut tasks = JoinSet::new();
            let (seen, mut received) = mpsc::unbounded_channel();
            let go = Arc::new(Notify::new());
            let packet_go = go.clone();
            env.spawn_packet(&mut tasks, async move {
                packet_go.notified().await;
                socket
                    .send_to("203.0.113.1:12345".parse().unwrap(), b"packet")
                    .await
                    .unwrap();
                let emitted = app_out.recv().await.unwrap();
                assert_eq!(emitted[0].as_ref()[9], 17); // Application stack emitted UDP.
                fwd_in.send(emitted).unwrap();
                let rejected = fwd_out.recv().await.unwrap();
                assert_eq!(rejected[0].as_ref()[9], 1); // Forwarder stack emitted ICMPv4.
                assert_eq!(rejected[0].as_ref()[20], 3); // Destination unreachable.
                app_in.send(rejected).unwrap();
                let inbound = app_raw.recv_bytes().await.unwrap();
                assert_eq!(inbound[20], 3); // Application ingress and polling delivered the reply.
                seen.send((thread::current().name().unwrap().to_owned(), Instant::now()))
                    .unwrap();
            });
            let (started, start) = oneshot::channel();
            let hog = tokio::spawn(async move {
                let now = Instant::now();
                started.send(now).unwrap();
                go.notify_one();
                while now.elapsed() < Duration::from_millis(500) {
                    std::hint::spin_loop();
                }
            });
            let start = start.await.unwrap();
            let (name, progressed) = received.recv().await.unwrap();
            assert_eq!(name, THREAD_NAME);
            assert!(progressed.duration_since(start) < Duration::from_millis(250));
            hog.await.unwrap();
            tasks.join_next().await.unwrap().unwrap();
            app.kill();
            fwd.kill();
            env.bus.kill();
            tokio::join!(
                app.wait_for_shutdown(),
                fwd.wait_for_shutdown(),
                env.bus.wait_for_shutdown()
            );
        })
        .await
        .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn enabled_metrics_sample_packet_thread_cpu() {
        if !crate::pipeline_metrics::enabled() {
            return;
        }
        let before = crate::pipeline_metrics::snapshot().thread_cpu_ns[0];
        let pipeline = PipelineThread::start().unwrap();
        pipeline
            .handle()
            .spawn(async {
                let start = Instant::now();
                while start.elapsed() < Duration::from_millis(20) {
                    std::hint::spin_loop();
                }
                tokio::time::sleep(Duration::from_millis(1100)).await;
            })
            .await
            .unwrap();
        assert!(crate::pipeline_metrics::snapshot().thread_cpu_ns[0] > before);
    }

    /// Run the public startup path in both suite modes, without a tailnet or real control server.
    #[tokio::test]
    async fn runtime_startup_honors_opt_in_and_drop_joins() {
        let control = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ts_control::Config {
            server_url: format!("http://{}/", control.local_addr().unwrap())
                .parse()
                .unwrap(),
            ..Default::default()
        };
        let runtime = tokio::time::timeout(
            Duration::from_secs(5),
            crate::Runtime::spawn(config, None, ts_keys::NodeState::generate()),
        )
        .await
        .unwrap()
        .unwrap();
        let opted_in =
            std::env::var_os("TS_EXPERIMENT_PIPELINE_THREAD").as_deref() == Some("1".as_ref());
        assert_eq!(runtime.env.packet_runtime.is_some(), opted_in);
        let task = if let Some(handle) = runtime.env.packet_runtime.as_ref() {
            assert_eq!(
                handle
                    .spawn(async { thread::current().name().unwrap().to_owned() })
                    .await
                    .unwrap(),
                THREAD_NAME
            );
            let (tx, rx) = oneshot::channel();
            let task = handle.spawn(async move {
                tx.send(()).unwrap();
                std::future::pending::<()>().await;
            });
            rx.await.unwrap();
            Some(task)
        } else {
            None
        };
        drop(runtime);
        if let Some(task) = task {
            assert!(task.await.unwrap_err().is_cancelled());
        }
    }

    #[tokio::test]
    async fn dropping_owner_on_packet_thread_does_not_self_join() {
        tokio::time::timeout(Duration::from_secs(2), async {
            let pipeline = PipelineThread::start().unwrap();
            let handle = pipeline.handle();
            let result = handle
                .spawn(async move {
                    drop(pipeline);
                })
                .await;
            assert!(result.is_ok() || result.unwrap_err().is_cancelled());
            // Owner drop signalled stop: the dedicated runtime must cancel future work too.
            loop {
                match handle.spawn(async {}).await {
                    Err(error) if error.is_cancelled() => break,
                    Ok(()) => tokio::task::yield_now().await,
                    Err(error) => panic!("{error}"),
                }
            }
        })
        .await
        .unwrap();
        // The caller runtime's blocking pool joins the OS thread; its shutdown also awaits that
        // joiner. This is the exceptional inline-callback drop path, not normal caller drop.
    }

    #[tokio::test]
    async fn drop_joins_and_cancels_tasks_even_if_they_await_the_caller() {
        struct Dropped(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let pipeline = PipelineThread::start().unwrap();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let (started, ready) = oneshot::channel();
        let task = pipeline.handle().spawn(async move {
            let _guard = guard;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        ready.await.unwrap();
        drop(pipeline);
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }
}
