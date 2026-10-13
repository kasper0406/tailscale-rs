//! Opt-in resources scoped to a shard and propagated to every spawned forwarding task.
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio::sync::{Semaphore, watch};

/// Node-wide admission shared by all shard runtimes. Port capacity is partitioned.
#[derive(Clone)]
pub struct ShardAdmission {
    /// Aggregate node-wide TCP splice budget.
    pub tcp: Arc<Semaphore>,
    /// Aggregate node-wide UDP flow budget.
    pub udp: Arc<Semaphore>,
    /// Partition of the existing 1,024 active-port budget.
    pub ports_per_shard: usize,
    /// Partition of the existing per-port TCP budget.
    pub tcp_per_port: usize,
    /// Partition of the existing per-port UDP budget.
    pub udp_per_port: usize,
}
impl ShardAdmission {
    /// Build shared node limits for 2..=64 fixed shards.
    pub fn new(shards: usize) -> Self {
        assert!((2..=64).contains(&shards));
        Self {
            tcp: Arc::new(Semaphore::new(crate::all_port::MAX_GLOBAL_TCP_SPLICES)),
            udp: Arc::new(Semaphore::new(crate::all_port::MAX_GLOBAL_UDP_FLOWS)),
            ports_per_shard: crate::all_port::MAX_PORTS / shards,
            tcp_per_port: crate::tcp::MAX_INFLIGHT_SPLICES / shards,
            udp_per_port: crate::udp::MAX_UDP_FLOWS / shards,
        }
    }
}
/// Per-shard task failure signal and live relay counts (only updated with metrics enabled).
#[derive(Clone)]
pub struct ShardResources {
    /// Shared node limits.
    pub admission: ShardAdmission,
    /// Any child panic signals the owning runtime supervisor.
    pub failed: watch::Sender<bool>,
    /// Optional count of active TCP splices and UDP reply pumps.
    pub flows: Option<Arc<AtomicUsize>>,
    /// Optional host-write attribution scoped to this shard.
    pub host_writes: Option<Arc<crate::splice_metrics::Counters>>,
}
tokio::task_local! { static RESOURCES: ShardResources; }
impl ShardResources {
    /// Enter on the shard runtime before constructing listeners or spawning tasks.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        RESOURCES.scope(self.clone(), future).await
    }
}
pub(crate) fn current() -> Option<ShardResources> {
    RESOURCES.try_with(Clone::clone).ok()
}
struct TaskGuard(Option<ShardResources>, bool);
impl Drop for TaskGuard {
    fn drop(&mut self) {
        if let Some(resources) = &self.0 {
            if self.1
                && let Some(flows) = &resources.flows
            {
                flows.fetch_sub(1, Ordering::Relaxed);
            }
            if std::thread::panicking() {
                resources.failed.send_replace(true);
            }
        }
    }
}
/// Capture at spawn time; Tokio task-local values are otherwise not inherited.
pub fn task<F: Future>(future: F, flow: bool) -> impl Future<Output = F::Output> {
    let resources = current();
    if flow && let Some(flows) = resources.as_ref().and_then(|r| r.flows.as_ref()) {
        flows.fetch_add(1, Ordering::Relaxed);
    }
    let guard = TaskGuard(resources.clone(), flow);
    async move {
        let _guard = guard;
        match resources {
            Some(resources) => resources.scope(future).await,
            None => future.await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn global_caps_and_per_port_caps_do_not_multiply_with_shards() {
        for n in [2, 4, 8, 64] {
            let admission = ShardAdmission::new(n);
            assert_eq!(admission.tcp.available_permits(), 2048);
            assert_eq!(admission.udp.available_permits(), 2048);
            assert!(admission.ports_per_shard * n <= 1024);
            assert!(admission.tcp_per_port * n <= 512);
            assert!(admission.udp_per_port * n <= 512);
            let (failed, _) = watch::channel(false);
            let mut held = Vec::new();
            for _ in 0..n {
                let r = ShardResources {
                    admission: admission.clone(),
                    failed: failed.clone(),
                    flows: None,
                    host_writes: None,
                };
                held.push(
                    r.scope(async {
                        current()
                            .unwrap()
                            .admission
                            .tcp
                            .acquire_many_owned((2048 / n) as u32)
                            .await
                            .unwrap()
                    })
                    .await,
                );
            }
            assert_eq!(admission.tcp.available_permits(), 0);
            assert!(admission.tcp.clone().try_acquire_owned().is_err());
            held.pop();
            assert_eq!(admission.tcp.available_permits(), 2048 / n);
        }
    }
    #[tokio::test]
    async fn detached_flow_panic_is_signalled_and_scope_propagates() {
        let (failed, rx) = watch::channel(false);
        let flows = Arc::new(AtomicUsize::new(0));
        let resources = ShardResources {
            admission: ShardAdmission::new(4),
            failed,
            flows: Some(flows.clone()),
            host_writes: None,
        };
        let task = resources
            .scope(async {
                tokio::spawn(super::task(
                    async {
                        assert!(current().is_some());
                        panic!("injected splice/reply-pump panic");
                    },
                    true,
                ))
            })
            .await;
        assert!(task.await.unwrap_err().is_panic());
        assert!(*rx.borrow());
        assert_eq!(flows.load(Ordering::Relaxed), 0);
    }
}
