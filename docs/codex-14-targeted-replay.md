# Brief 14: opt-in targeted waiter replay

2026-10-12. Built and validated in `/Volumes/git/ts-exit/scratch/wt-codex-scans`,
branch `ts-exit/netstack-scans`, starting at `497b5fd`. No push. Claude benchmarks
the same binary with `TS_EXPERIMENT_TARGETED_REPLAY` unset versus `=1`.

| Commit | Change |
|---|---|
| `0cafbfe` | `ts_netstack_smoltcp_core: add opt-in targeted waiter replay` |

This document is committed separately. Both commits carry the brief 13
Signed-off-by and GPT-6.1 Co-Authored-By trailers.

**Design.** The core enables the vendored smoltcp `async` feature without new
crates or a lockfile change. With the experiment disabled, blocked commands
continue through the existing global FIFO, generation guard, cancellation skip
and replay accounting. The environment flag is cached once per process at the
first stack construction, like the metrics flag. Restart the process to change
modes. Builds without the core's `std` feature leave the experiment disabled;
runtime builds enable that feature.

With the experiment enabled, TCP Recv, Send and Connect and UDP Recv/Send wait
in FIFOs keyed by `(SocketHandle, generation)`. Each FIFO shares one custom
waker across all its waiters: receive registers RX, send registers TX, connect
registers both. Every requeue rearms the applicable one-shot registration. The
waker only sets an atomic dirty bit and sends the key on an existing flume
channel on the first clean-to-dirty transition. This forms a shared deduplicated
dirty set without a new mutex dependency, unsafe code or a pointer to the stack.

Pumps snapshot dirty notifications and replay each indicated FIFO in order,
attempting its original entries once. Requeues append behind the original
entries. Wakes produced during replay wait for the next pump. Generation checks
precede execution; disconnected receives are skipped before consuming data.
Socket removal drops its FIFO and answers remaining live commands with
`missing_socket`. A late waker for an old generation cannot replay a recycled
slot's new queue. Handle-less commands, listeners/accept and raw/ICMP retain
always-replay behavior. Listener promotion still runs at the existing ingress
points and before the ordinary waiter pump. FIN, Close and ShutdownWrite retain
the existing delivery path.

A full safety pass runs at most once per 50 ms while queues are nonempty. Dirty
queues are drained first, so normal readiness completions are attributed to
targeted replay. `poll_at`/`poll_delay` include this deadline, even for UDP-only
stacks, while preserving earlier TCP and orphan deadlines. Both synchronous I/O
and `IoPoller` service the deadline, including timer turns without device
capacity or packet progress. Earlier/spurious timer turns pump readiness; they
do not force additional full sweeps inside the 50 ms cadence. As with the
existing maintenance timer, the bound requires servicing the driver and adds
any driver scheduling delay.

Normal enqueue purges that handle's abandoned receives. During replay, each
original entry receives the cancellation check instead, preventing a purge
from shortening the queue and accidentally replaying a requeued entry twice.
The full pass also purges canceled receives. A never-waking socket with a
serial timeout/retry loop retains at most the latest canceled receive between
operations, rather than accumulating one per timeout. Disconnected sends and
other commands retain their existing delivery semantics.

**Design adjustments.** Empty FIFO allocations and their cached wakers stay
attached to the socket generation until removal. They do not arm the safety
timer or count as dirty handles pumped. This avoids allocating a map node, Arc
and deque on each receive; the periodic pass filters empty entries.

The first full targeted run exposed brief 13's failed-connect ordering rule:
waking a reset connect before reclamation returned `ConnectionReset`, whereas
the existing IPv6 first-SYN rejection requires `BadSocketHandle`. Targeted
connect replay now checks for rx-empty Closed and reclaims that socket before
answering `missing_socket`. This readiness check counts as an attempted and
completed replay, including when recovered by the safety net. The regression
and a deliberately suppressed reset wake are covered by tests. Ordinary
removal responses, stale-generation rejection and abandoned receives do not
count as executed replays.

**Counters.** Set `TS_DEBUG_PIPELINE_METRICS=1` at startup for attribution.
The existing process-wide core snapshot, runtime snapshot and exit-node
cumulative status log expose the additional fields automatically:

| Field | Meaning / expected production behavior |
|---|---|
| `dirty_handles_pumped` | Dirty generations with pending commands actually pumped; spurious wakes can count. |
| `targeted_replay_attempts` | Commands replayed through dirty FIFOs, including failed-connect readiness checks. |
| `targeted_replay_completions` | Those replays returning terminal responses, including errors. |
| `safety_net_sweeps` | Full passes over nonempty targeted queues; at most about 20/s per serviced stack. |
| `safety_net_attempts` | Commands actually attempted during those passes, after dirty work drains. |
| `safety_net_completions` | Terminal safety responses; expected approximately zero under normal load. Investigate nonzero deltas as missed readiness paths. |

The old replay totals include global fallback, targeted and safety work.
Validation lookups include targeted registration checks, cache cleanup and
queue removal. With metrics disabled all process counters remain zero. In
production with the experiment disabled the six new counters remain zero.
With targeted mode enabled, attempts should track readiness notifications and
waiter count rather than every packet-progress pump times all flows. Spurious
wakes and opposite-direction waiters prevent an exact 1:1 attempt/completion
ratio. Safety attempts can remain nonzero for legitimately blocked queues:
approximately pending commands times the 20 Hz cadence, independent of packet
pump rate. The tests intentionally generate safety completions for suppressed
RX and reset wakes; those are expected test-only values.

**Validation.** All Cargo commands used `+1.95.0`. The final brief 13 suite
passed **749 tests across 20 groups in each mode**, including all 597 runtime
tests, with zero failures or ignored tests:

```sh
cargo +1.95.0 test \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp \
  -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime \
  --features geiserx_ts_runtime/ring-aead

TS_EXPERIMENT_TARGETED_REPLAY=1 cargo +1.95.0 test \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp \
  -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime \
  --features geiserx_ts_runtime/ring-aead
```

- Three extra consecutive `ts_forwarder --test forwarding` runs per mode:
  16/16 each, 96 additional test executions, including half-close/EOF delivery.
- Metrics-enabled core tests with the experiment unset and set: 54/54 each.
- Core `--lib --no-default-features` check and exit-node example check with
  `ring-aead`: passed.
- Core library Clippy with `--features std -- -D warnings`: passed.
  All-target Clippy passed with `-D warnings -A clippy::drop_non_drop`; fully
  strict all-target Clippy reports two existing `drop(io)` calls in brief 13's
  untouched idle-reclamation test. No production lint exception was needed.
- Nightly rustfmt check on only the four touched Rust files and
  `git diff --check`: passed.

The core has 48 unit tests, up from 33. Fifteen new tests exercise real
loopback packet exchange for connect/accept, data arrival, FIFO and one-shot
rearming, ACK freeing TX space at the 10 ms delayed-ACK timer, remote FIN,
remote RST/local Close and UDP receive. Further cases cover removal and late
wakes across slot reuse, stale generations, 1,000 canceled receive retries,
50 ms recovery of suppressed data and reset wakes, UDP-only deadlines, idle
async recovery with neither RX nor TX capacity, budgeted async ingress and
the environment gate. Existing tests inspecting the global queue explicitly
select legacy mode; the new readiness tests explicitly select targeted mode
in both invocations. Integration and runtime tests use the environment flag.

Logs are `/tmp/codex-14-{default,targeted}-tests.log`,
`/tmp/codex-14-forwarding-{default,targeted}-{1,2,3}.log`,
`/tmp/codex-14-metrics-{default,targeted}.log`,
`/tmp/codex-14-{no-std-check,example-check,clippy-lib,clippy-targets}.log`.
`/tmp/codex-14-validation.json` summarizes the final supplemental checks.
`bd prime` failed with `bd: command not found`; no tracker mutation or sync
was attempted.

**Risks and benchmark handoff.** A missed wake adds up to 50 ms plus driver
scheduling delay. Empty queue caches retain metadata and FIFO capacity until
socket removal. Dirty notifications add atomic/channel work and a temporary
batch allocation; targeted replay still performs logarithmic map lookups.
The full safety pass remains O(live cached socket queues + pending commands)
at its bounded cadence. Listener/global fallback replay and smoltcp's egress,
matching and timer scans remain. Enabling smoltcp async adds waker fields and
wake checks even in default mode; only targeted mode registers these wakers.

Claude's supplied baseline measured about 120 attempts per completion and
99.2% wasted replay work after item 1. This implementation addresses that
volume, but no CPU/GB or throughput result is claimed here. A/B the same
binary with the experiment unset/set, keeping metrics-disabled performance
runs separate from metrics-enabled attribution. Check safety completions,
attempt/completion deltas, finite-transfer EOF, cancellation/churn, latency,
retransmissions and memory. Apply the existing 15% high-flow CPU/GB gate and
single-flow/throughput/delivery limits to measured results.
