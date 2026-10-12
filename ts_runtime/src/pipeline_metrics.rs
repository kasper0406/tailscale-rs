//! Process-wide experimental packet counters. Disabled unless TS_DEBUG_PIPELINE_METRICS=1.
//! Record once per batch/drain, never format on the hot path. Histograms have exact buckets 0..64.
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

static ENABLED: OnceLock<bool> = OnceLock::new();
static DROPS: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static EGRESS: [[AtomicU64; 65]; 2] = [const { [const { AtomicU64::new(0) }; 65] }; 2];
static BUSY: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// Whether experimental cumulative counters are enabled for this process.
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("TS_DEBUG_PIPELINE_METRICS").as_deref() == Ok("1"))
}

pub(crate) fn stack_index(name: &str) -> usize {
    usize::from(name == "forwarder")
}
pub(crate) fn drops(index: usize, n: u64) {
    DROPS[index].fetch_add(n, Ordering::Relaxed);
}
pub(crate) fn egress(index: usize, n: usize) {
    EGRESS[index][n.min(64)].fetch_add(1, Ordering::Relaxed);
}

/// Cheap cumulative snapshot; array order: app/forwarder and pipeline/security.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Opt-in kernel receive batch distribution, fallback and truncation counts.
    pub receive: ts_magicsock::ReceiveBatchSnapshot,
    /// Cumulative work across application and forwarder netstacks.
    pub netstack: ts_netstack_smoltcp::netcore::pipeline_metrics::Snapshot,
    /// Packets refused by netstack admission (queue capacity or gate limits), indexed app/forwarder.
    pub ingress_drops: [u64; 2],
    /// Number of egress batches of each size, indexed app/forwarder then exact size 0..64.
    pub egress_batches: [[u64; 65]; 2],
    /// Accumulated thread CPU nanoseconds, sampled once per second (not elapsed wall time).
    pub thread_cpu_ns: [u64; 2],
    /// Completed sendmsg_x calls by number of datagrams sent, exact size 0..64.
    pub sendmsg_x_batches: [u64; 65],
    /// Attempted sendmsg_x calls by number offered; includes failures and retries.
    pub sendmsg_x_offered_batches: [u64; 65],
}
/// Read counters without actor messaging or hot-path tracing.
pub fn snapshot() -> Snapshot {
    Snapshot {
        receive: ts_magicsock::receive_batch_snapshot(),
        netstack: ts_netstack_smoltcp::netcore::pipeline_metrics::snapshot(),
        ingress_drops: core::array::from_fn(|i| DROPS[i].load(Ordering::Relaxed)),
        egress_batches: core::array::from_fn(|i| {
            core::array::from_fn(|j| EGRESS[i][j].load(Ordering::Relaxed))
        }),
        thread_cpu_ns: core::array::from_fn(|i| BUSY[i].load(Ordering::Relaxed)),
        sendmsg_x_batches: ts_magicsock::send_batch_histogram(),
        sendmsg_x_offered_batches: ts_magicsock::send_batch_offered_histogram(),
    }
}

pub(crate) fn start_cpu_sampler(name: &str) {
    if !enabled() {
        return;
    }
    let index = usize::from(name == "ts-security-guard");
    tokio::spawn(async move {
        let mut last = thread_cpu_ns();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            let now = thread_cpu_ns();
            BUSY[index].fetch_add(now.saturating_sub(last), Ordering::Relaxed);
            last = now;
        }
    });
}
#[cfg(unix)]
fn thread_cpu_ns() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid writable timespec; CLOCK_THREAD_CPUTIME_ID measures this sampler's OS thread.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) } != 0 {
        return 0;
    }
    (time.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(time.tv_nsec as u64)
}
#[cfg(not(unix))]
fn thread_cpu_ns() -> u64 {
    0
}
