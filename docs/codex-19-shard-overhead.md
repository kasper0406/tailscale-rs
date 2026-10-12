# Brief 19: sharded upload overhead

Worktree: `scratch/wt-codex-shard`, branch `ts-exit/shard-stage1`, base
`179e151`. No existing experiment defaults changed. No shared live node was
restarted, peer configuration rewritten, or branch pushed during this work.

The supplied upload profile identifies a shared ownership-ledger mutex on the
per-packet path, small host writes, and small return-path WG/UDP batches. The new
counters confirm unnecessary queue publications and bounded-WG backpressure in
the local fixture. Four independently selectable treatments remove that work.
They do **not** yet establish how much of the real additional 5.86 CPU-s/GB they
recover. The complete UDP/control harness A/B remains for Claude; the commands
below use the same final node binary for all three variants.

The single-flow download investigation found an important probe mismatch. With
the synthetic generator's TCP-driver policy reproduced, the local N=4 download
gap almost disappears. The real generator is near one CPU core in the supplied
runs, while the node uses less CPU at N=4. This implicates generator/UDP/burst
interaction, but does not prove which generator call path causes the real
4.35 -> 2.73 Gbit/s regression. A generator profile is still required.

## Supplied full-harness baseline

Medians from `bench/results/stage0/coal-co1/` and `coal-co4/`, coalesce,
targeted replay, RX batching and ACK batching enabled. CPU is **node-only**,
decimal GB. These are pre-treatment measurements, not new runs.

| Load | Direction | N=1 Gbit/s | N=1 CPU-s/GB | N=4 Gbit/s | N=4 CPU-s/GB |
|---|---|---:|---:|---:|---:|
| 1×1 | down | 4.348 | 1.92 | 2.732 | 1.84 |
| 1×1 | up | 2.558 | 5.18 | 2.534 | 5.61 |
| 2×8 | down | 7.484 | 2.29 | 6.309 | 2.21 |
| 2×8 | up | 4.042 | 5.78 | 2.950 | 10.86 |
| 2×64 | down | 5.682 | 4.11 | 5.625 | 3.07 |
| 2×64 | up | 3.013 | 7.87 | 2.222 | 13.73 |

The metrics-on attribution log `scratch/co4-up-metrics.txt` has 4,166,121 shard
packets admitted, one dispatcher/pump drop, and 1,840,491 output packets (0.442
output/input). Dispatcher loss cannot explain this run's upload overhead.
This is a cumulative snapshot, including startup; it is not a precisely matched
before/after transfer delta.

Host writes total 5,120,000,128 bytes in 3,314,552 successful nonzero writes:
mean 1,544.7 bytes, 85.27% at most 2 KiB, 97.82% at most 4 KiB. Only 418
Pending polls and 325 partial completions were recorded. Coalescing therefore
still receives small ready chunks; host socket capacity is rarely the reason
for splitting them. There were 558,859 forwarder output batches, mean 3.29
packets, 37.79% single-packet batches. The 377,627 `sendmsg_x` completions average
4.13 datagrams; 70.45% fall in the 2–4 datagram bucket. Cross-shard ready merging
has room to reduce batch turns. It does not avoid encrypting each datagram.

## Per-thread graphs and mutex attribution

The same-load `scratch/prof/co4-up-metrics.txt` contains 4,162 sampled instants
per thread. Counts below are inclusive where marked; they overlap and cannot be
added as CPU time. Syscall/parking columns are leaf samples. `sample` observes
wall-time stacks, including waits, so these counts cannot assign exact shares
of the extra CPU-s/GB.

| Thread | WG step incl. | WG inbound incl. | WG outbound incl. | Direct send incl. | recvmsg_x | sendmsg_x | UDP sendto | mutexwait | park cv | kevent |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| worker 0 | 729 | 544 | 150 | 412 | 297 | 250 | 40 | 33 | 1714 | 706 |
| worker 1 | 851 | 641 | 162 | 411 | 264 | 246 | 53 | 34 | 1562 | 779 |
| worker 2 | 753 | 554 | 157 | 423 | 291 | 258 | 63 | 29 | 1658 | 751 |
| worker 3 | 777 | 599 | 140 | 422 | 249 | 239 | 60 | 38 | 1630 | 786 |

| Thread | Host TCP sendto | Ingress pump incl. | poll_delay incl. | mutexwait | kevent |
|---|---:|---:|---:|---:|---:|
| shard 0 | 1027 | 445 | 73 | 117 | 1305 |
| shard 1 | 760 | 299 | 55 | 72 | 1934 |
| shard 2 | 960 | 373 | 74 | 73 | 1584 |
| shard 3 | 1136 | 448 | 66 | 107 | 1178 |

The important callers are:

- Shard: Tokio task poll -> `forwarder_shards::pump` -> ownership-ledger lock;
  netstack stream/splice -> Tokio host `TcpStream::poll_write` -> `__sendto`.
- Worker: `DataPlane::step` -> `process_inbound` -> authenticated dispatcher;
  `DataPlane::step` -> `process_outbound` -> ownership-ledger lock/encryption;
  `direct::run_direct` -> MagicSock -> `sendmsg_x` or UDP `__sendto`.
- Stack timer/poll: netstack `poll_delay` -> socket-set deadline scan. Its 268
  inclusive shard samples include only 17 exclusive `poll_delay` samples in the
  aggregate profile summary. This is measurable work, not demonstrated to be
  the principal cause.

Of 503 `__psynch_mutexwait` leaf samples, 369 are shard ownership-ledger waits,
48 are WG outbound ownership-ledger waits, 79 are worker parking, one scheduler
notification, and six MagicSock locks. Thus 417/503 belong to ownership callers.
The shard lock is inlined: disassembly of the profiled
`bin/exit_node-coalesce2` maps return PC `0x10032e048` (image load
`0x10025c000`, unslid `0x1000d2048`) to the pump lock followed by the ledger's
8,192-entry limit check. It is the ownership mutex, not the source/ACL mutex.
Parsed graphs and this disassembly are retained in
`/tmp/codex-19-profile.json` and `/tmp/codex-19-pump-disassembly.txt`.

## Opt-in treatments

`TS_EXPERIMENT_SHARD_BATCH` is cached at startup. Unset, `0`, and unrecognized
values retain the old path; `1` enables all four treatments. Component values
allow attribution without changing any existing flag defaults:

| Value | Treatment |
|---|---|
| `dispatch` | Group authenticated ingress by shard, publish up to 64 packets per message |
| `owners` | Cache successfully registered source/peer ownership within each shard |
| `wg` | Drain already-ready bounded WG publications across shards into at most 64 packets per turn |
| `egress` | Try publishing stack output directly to the bounded WG mailbox; retain the existing pipe/pump fallback |
| `1` | All of the above |

Dispatch batching preserves packet order within each shard and the original
peer identity. It does not add a fill timer. In addition to the existing
4,096-message channel and 4,096×1,280-byte budget, batched mode has an independent
4,096-packet atomic budget, covering queued and currently consumed packets.
RAII releases both budgets on processing, rejection, or receiver drop. The
ordinary one-packet path retains its allocation-free message representation.

Ownership caching skips only repeated global ledger locks/inserts. The ledger
is append-only and capped at 8,192 entries; each shard's cache is consequently
also bounded by 8,192 entries. **Every packet**, including a cache hit, still
passes source and ACL revalidation under the security mutex immediately before
admission. Source removal/reassignment of a recorded owner stops the whole group
before barrier acknowledgement, as in brief 17. The WG outbound ownership check
remains unchanged. No source/ACL or fail-closed bypass is introduced.

WG merging first waits for one message, then takes only messages already ready,
up to 64 messages and 64 output packets per turn. A partly used producer batch
has at most 63 retained packets (production producers emit at most 64). FIFO and
closure draining are preserved. A lone packet returns immediately. The normal
Tokio select and routing, ownership validation, encryption and replay behavior
remain in place. Every output packet is still individually sealed.

Inline egress reserves a bounded WG slot before converting packet buffers. On
full or closed WG capacity, it returns the original buffers to the existing
bounded output pipe. A `lagging` latch forces every later batch through that
pipe until the colocated current-thread pump has sent its current batch and
emptied the receiver, including buffered tails. This prevents newer inline
output overtaking an older fallback batch. A closed WG channel still terminates
the pump and triggers existing supervision. The bounded device-output budget
is released when ownership transfers successfully; the WG queue remains bounded
at 64 publications. N=1 does not install this sink.

No timer interval, TCP window, socket count, proxy policy, destination guard,
fragment policy, global failure behavior, or existing experiment default was
changed. The pipe also now releases its charged batch budget if its consumer
has closed, matching the existing single-packet error path.

## Metrics and observed work reduction

All new counters and Waker adapters are behind `TS_DEBUG_PIPELINE_METRICS=1`.
The disabled path does not wrap channel Wakers or allocate per-stack metric
handles. Snapshots are cumulative: subtract before/after snapshots over a
matched transfer window and keep attribution separate from metrics-off CPU A/B.

Per-shard snapshots now include exact dispatch and egress histograms (buckets
0–64), actual receiver Waker callbacks, receiver polls/Pending, ownership-lock
acquisitions, ingress flushes, WG mailbox waits, inline publications, successful
host-write sizes/bytes/Pending/partial writes, and local stack poll/replay/
maintenance/deadline-scan counters. Global `wg_handoff` reports bounded WG
messages received, merged-turn histogram, receiver callbacks/polls, and offered
outbound packet shape for both N=1 and N>1. IPv4 TCP payload histogram bounds
are 64/256/512/1024/1220/1460/4096/unbounded bytes; empty payloads and fragments
are excluded, retransmissions count again. These offered packets are counted
**before** routing/ownership rejection, not as successful encryption or send.

A publication is a receiver-notification **attempt**, not necessarily an actual
Waker callback. Waker callbacks can coalesce and include cooperative-budget
wakeups; they are not OS thread unparks. `wg_waits` counts each send once if it
returns Pending while the bounded sender has zero slots, rather than counting
all Pending retries. Histograms at the shard seam measure each shard's output;
the WG histogram measures the resulting combination across shards.

The local metrics-on, full-WG upload control transfers 512 MB across 128 flows,
one authenticated peer, MTU 1500. This fixture shares its process/runtime with
the peer and sink and uses the initial batched peer-driver policy. Counters are
matched before/after deltas; these single runs attribute work, not throughput.
Logs: `/tmp/codex-19-attribution.json` and its referenced per-run files.

| Counter | N=4 old path | N=4 all treatments |
|---|---:|---:|
| admitted packets | 352,115 | 354,166 |
| dispatch publications | 352,115 | 19,789 |
| dispatch receiver callbacks | 14,088 | 13,641 |
| dispatch receiver polls | 366,178 | 33,429 |
| ownership-lock acquisitions | 352,115 | 3 |
| output packets | 127,070 | 98,560 |
| output publications | 9,409 | 15,516 |
| WG receive turns | 9,409 | 4,778 |
| WG full-mailbox waits | 4,919 | 0 |
| inline publications | 0 | 15,516 |
| WG receiver callbacks | 109 | 3,051 |
| stack I/O polls | 80,454 | 76,785 |
| poll_delay scans | 46,452 | 46,121 |
| orphan / safety sweeps | 108 / 211 | 100 / 130 |

Dispatch publications fall to 0.056 per admitted packet, but actual callbacks
remain about 0.04 per packet. This demonstrates channel/poll work reduction,
not a per-packet OS wake storm. The fourth shard's ownership entry was already
warmed by the echo setup; three new locks cover the other shards. WG callbacks
increase when the queue can become idle instead of remaining continuously ready.
The number of emitted TCP ACKs differs, so compare both packet-normalized counts
and absolute work, not just total turns.

Individual controls isolate the mechanisms: dispatch-only produces 14,829
messages for 352,129 packets; owners-only acquires three locks; WG-only merges
12,950 messages into 4,644 turns and records zero full-mailbox waits;
egress-only publishes 3,183 batches inline but still records 4,914 mailbox
waits. Merging is needed for inline publication to work consistently. Timer/
scan totals barely change under the combined treatment; no timer fix is justified
by this evidence. Host-write histograms by shard are now available for the real
load; the supplied old profile cannot retroactively provide per-shard sizes.

## Probe results and the download check

The ignored `local_splice_probe` can now run both real WG endpoints, production
source/ACL admission, bounded sharded output, and real host TCP sockets, with
payload and half-close verification. Its **encrypted transport is in memory**:
no UDP, MagicSock, control, or separate generator process. CPU uses
`CLOCK_PROCESS_CPUTIME_ID` and includes node, peer, sink and validation, not
node-only CPU. Non-Unix builds report zero CPU time. Thus it is useful for
mechanisms and controls, but cannot replace the full-harness table above.

Initially the peer fixture inherited the node's RX/ACK batching and bounded
pipe policy. The real `examples/synthetic_peer.rs` instead uses an unbounded
pipe, MTU 1280, work budget 128, no ACK batching, per-packet ingress wake, and
yields at each WG/output turn. `TS_BENCH_SPLICE_PEER_MODE=harness` now matches
those driver policies; the runtime still has four shared workers rather than
two workers in each separate real generator. The probe's MTU default is now
1280, with explicit overrides for comparison to its original MTU-1500 runs.
The generator binary and defaults were not changed.

Three interleaved, metrics-off single-flow download controls, 128 MB, MTU 1280,
zero ingress drops (`/tmp/codex-19-policy-probes.json`):

| Peer driver | N=1 old Gbit/s / process CPU-s/GB | N=4 old | N=4 all |
|---|---:|---:|---:|
| Initial batched node-style peer | 4.671 / 2.856 | 3.772 / 2.542 | 3.969 / 2.494 |
| Synthetic-generator policy | 5.119 / 2.971 | 5.012 / 2.955 | 5.145 / 2.991 |

With the generator policy, offered TCP payload deltas are exactly 128,000,001
bytes in 103,517 data packets for all three variants, mean 1,236.5 bytes.
Smaller TCP segments do not explain a gap in this control. Inline-egress-only
controls with the earlier peer policy improved download about 3%, leaving the
larger discrepancy. A window sweep (64 KiB/256 KiB/1 MiB) also retained the
N=4 relative gap at larger windows. Neither warrants changing TCP-buffer or
scheduler defaults. Under-load echo in that earlier fixture rose from about
308 to 365 microseconds p50 (all treatments 359), despite idle echo rising only
about 8 microseconds. Idle echo latency alone therefore does not characterize
the bulk ACK loop.

For comparison, the original metrics-off MTU-1500, node-style-peer matrix
(`/tmp/codex-19-final-probes.json`, three interleaved repeats) measured:

| Total flows | Direction | N=1 old Gbit/s / process CPU-s/GB | N=4 old | N=4 all |
|---|---|---:|---:|---:|
| 1 | down | 4.755 / 2.747 | 4.016 / 2.492 | 4.209 / 2.442 |
| 1 | up | 1.387 / 8.385 | 1.387 / 8.558 | 1.392 / 8.477 |
| 16 | down | 7.825 / 2.833 | 9.402 / 3.042 | 7.792 / 3.137 |
| 16 | up | 2.407 / 12.696 | 3.754 / 9.464 | 4.998 / 7.951 |
| 128 | down | 7.483 / 3.834 | 2.562 / 4.513 | 2.569 / 4.654 |
| 128 | up | 2.008 / 13.967 | 1.694 / 21.860 | 1.474 / 17.224 |

These are **one-peer** flow-count surrogates for 1×1/2×8/2×64. At 16 flows the
combined treatment reduces upload process CPU/GB 16% and improves rate 33%,
but download falls 17%. At 128 flows it lowers upload CPU/GB 21% while rate
falls 13%. Peer ingress drops across both directions/three repetitions total
6,626 / 18,497 / 73,120 for N=1 / N=4 old / N=4 all. Those overloaded fixture
runs include retransmission effects and do not establish a general win or
justify enabling the treatment by default.

In the **real** single-flow download table, generator CPU/GB increases from
1.659 to 2.842 while generator CPU use stays near one core (0.906 -> 0.971).
Node CPU use drops from 1.047 to 0.623 cores and node CPU/GB drops slightly.
This is consistent with a serial generator receive/decrypt/TCP-ACK driver
ceiling under changed UDP batch/burst shape. It is an inference, not a sampled
generator attribution. The in-memory policy control excludes an unavoidable
shard-hop or TCP-segmentation ceiling but omits the UDP loop that could produce
the real regression. New offered-packet histograms and per-shard/WG turn counts
make that distinction testable in the full harness.

Attempts to sample our child probes failed three times: macOS `sample` returned
255, “cannot examine process ... unknown reasons ... try sudo.” Transfers still
passed. No privileged sampling was attempted. See
`/tmp/codex-19-sample-probe.log`. The supplied existing node profile remains the
source for the per-thread call graphs above.

## Validation

Used Rust 1.95.0 and `--locked`, without dependency changes. The full suite over
netstack core/socket/wrapper, forwarder, runtime, MagicSock, tunnel and dataplane
with `geiserx_ts_runtime/ring-aead` passed **1,115 tests in each of 12 fresh
process modes**, 29 result groups per mode:

- Default; targeted/RX/ACK + metrics + connected/batched UDP/DF flags.
- Coalesce N=4 baseline; N=1 with all treatments.
- Coalesce N=4 with each of `1`, `dispatch`, `owners`, `wg`, `egress`.
- N=4 all treatments with RX/ACK/metrics/coalesce flags absent.
- N=4 all treatments with full netstack budget; N=1 pipeline thread with treatment flag.

The final matrix log is `/tmp/codex-19-tests-final2.log`; per-mode test output is
`/tmp/codex-19-all-*.log`. The final packet-shape instrumentation was also checked
with metrics/all treatments enabled in `/tmp/codex-19-final-metrics-tests.log`.

Added tests cover packet and byte caps independently, batched FIFO/peer identity,
RAII receiver-drop accounting, source revocation after the 128-packet yield
boundary (128 accepted/64 rejected, including ownership cache hits), ready-WG
merge/tail/closure/no-fill-wait, actual receiver callback observation, successful
inline budget release, full/closed fallback, and FIFO latch behavior. Existing
cold TCP/UDP on every shard, both TCP half-closes, source reassignment, security
fanout, panic/failure global stop and thread join tests pass. The integration
setup uses a bounded mock WG sender in inline modes so those paths are exercised.

Extra forwarding integration tests passed 3 repetitions of N=1/N=4 × coalesce
off/on. Core no-std, socket futures-io, and Linux cross-checks of core/socket/
wrapper passed. Production library Clippy and the `exit_node` example pass with
`-D warnings`. All-targets Clippy passes with two existing test-only lints
allowed (`drop_non_drop`, `async_yields_async`); without those allowances it
fails on pre-existing tests. No unrelated lint edits were retained. Touched-file
Rustfmt and `git diff --check` pass. The final optimized node and probe build
with profile `exitnode`.

`bd prime` was attempted, but `bd` is not installed (`command not found`). No
tracker update or sync was possible. The user's explicit commit/no-push request
controls the repository handoff.

## Small full-harness A/B for Claude

Keep the **same synthetic_peer binary/hash and peer policies** across variants.
Use the final node binary throughout, workers 4, TCP buffers 262144, coalesce
on, targeted replay/RX/ACK on, connected/batched UDP/DF on, pipeline thread and
full netstack budget off. Metrics are off for CPU measurements.

| Variant | Shards | SHARD_BATCH | Required cells |
|---|---:|---|---|
| A | 1 | `0` | 1×1, 2×8, 2×64; down and up |
| B | 4 | `0` | same |
| C | 4 | `1` | same |

Three repetitions per cell: 54 transfers. The runner uses 2 GB/flow for 1×1,
250 MB/flow for 2×8, 30 MB/flow for 2×64 and two workers per generator, with
one control holder per peer and 15 seconds settling. Record node-only and
generator CPU/GB, rate, drop deltas, and WG/UDP batch shape. The pre-treatment
full-harness table is evidence; post-treatment full-harness cells remain unrun.

Build/stage (does not restart the live node):

```sh
cd /Volumes/git/ts-exit/scratch/wt-codex-shard
cargo +1.95.0 build --locked --profile exitnode --example exit_node --features ring-aead
cp target/exitnode/examples/exit_node /Volumes/git/ts-exit/bin/exit_node-shard19
cd /Volumes/git/ts-exit
shasum -a 256 bin/exit_node-shard19 bin/synthetic_peer
```

These next commands **restart the shared node and refresh peer disco configs**;
run them in Claude's coordinated harness window. `bench-node.sh` keeps connected/
batched UDP and DF on; the example's existing defaults are four workers and
256 KiB TCP buffers. Do not run another benchmark or build alongside a load.

```sh
cd /Volumes/git/ts-exit
runner/bench-node.sh bin/exit_node-shard19 TS_EXPERIMENT_FORWARDER_SHARDS=1 TS_EXPERIMENT_SHARD_BATCH=0 TS_EXPERIMENT_SPLICE_COALESCE=1 TS_EXPERIMENT_TARGETED_REPLAY=1 TS_EXPERIMENT_RX_BATCH=1 TS_EXPERIMENT_ACK_BATCH=1 TS_EXPERIMENT_NETSTACK_BUDGET=0 TS_EXPERIMENT_PIPELINE_THREAD=0 TS_DEBUG_PIPELINE_METRICS=0
runner/stage0-matrix.sh shard19-a bench/results/stage0/shard19-a p1f1 p2f8 p2f64
runner/bench-node.sh bin/exit_node-shard19 TS_EXPERIMENT_FORWARDER_SHARDS=4 TS_EXPERIMENT_SHARD_BATCH=0 TS_EXPERIMENT_SPLICE_COALESCE=1 TS_EXPERIMENT_TARGETED_REPLAY=1 TS_EXPERIMENT_RX_BATCH=1 TS_EXPERIMENT_ACK_BATCH=1 TS_EXPERIMENT_NETSTACK_BUDGET=0 TS_EXPERIMENT_PIPELINE_THREAD=0 TS_DEBUG_PIPELINE_METRICS=0
runner/stage0-matrix.sh shard19-b bench/results/stage0/shard19-b p1f1 p2f8 p2f64
runner/bench-node.sh bin/exit_node-shard19 TS_EXPERIMENT_FORWARDER_SHARDS=4 TS_EXPERIMENT_SHARD_BATCH=1 TS_EXPERIMENT_SPLICE_COALESCE=1 TS_EXPERIMENT_TARGETED_REPLAY=1 TS_EXPERIMENT_RX_BATCH=1 TS_EXPERIMENT_ACK_BATCH=1 TS_EXPERIMENT_NETSTACK_BUDGET=0 TS_EXPERIMENT_PIPELINE_THREAD=0 TS_DEBUG_PIPELINE_METRICS=0
runner/stage0-matrix.sh shard19-c bench/results/stage0/shard19-c p1f1 p2f8 p2f64
```

For time drift control, prioritize repeating/interleaving B/C before enlarging
the matrix. If a regression appears, use the individual component values to
identify it; do not enable the combined treatment as a default based on the
counter reductions alone.

For same-load attribution, make an **up-only temporary copy** of the runner,
with 40 MB/flow (5.12 GB total), one repetition and a ten-second node sample:

```sh
python3 - <<'PY'
from pathlib import Path
root = Path('/Volumes/git/ts-exit')
source = (root / 'runner/synthetic-matrix.py').read_text()
needle = 'for direction in ("down", "up"):'
assert source.count(needle) == 1
source = source.replace(needle, 'for direction in ("up",):')
Path('/tmp/codex-19-up-only.py').write_text(
    'import sys\nsys.path.insert(0, "/Volumes/git/ts-exit/runner")\n' + source)
stage = (root / 'runner/stage0-matrix.sh').read_text()
assert '30000000 3' in stage
stage = stage.replace('runner/synthetic-matrix.py', '/tmp/codex-19-up-only.py')
stage = stage.replace('30000000 3', '40000000 1')
Path('/tmp/codex-19-up-only.zsh').write_text(stage)
PY
# Restart as B or C above, but with STATUS_INTERVAL=1 and TS_DEBUG_PIPELINE_METRICS=1.
# After the restart:
zsh /tmp/codex-19-up-only.zsh shard19-attr-b bench/results/stage0/shard19-attr-b p2f64 &
load_pid=$!
# Holders settle for 15 seconds; check log that the transfer is active when sampling.
sleep 20
sample "$(cat state/rust-exit/exit_node.pid)" 10 1 -file scratch/prof/shard19-attr-b.txt
wait "$load_pid"
cp state/rust-exit/exit_node.log scratch/shard19-attr-b-node.log
```

Repeat for C; change both labels/path names. Subtract the status snapshots
immediately before/after the transfer, inspect each shard's host-write and
output histograms, and compare dispatch publications **and** Waker callbacks,
WG turn sizes/full waits, owner locks and deadline/maintenance scans. Reuse
these single-transfer controls for `dispatch`, `owners`, `wg`, `egress` if
needed. Metrics-on CPU values are attribution controls, not the matrix score.

For single-flow download attribution, profile **both** the node and generator
while measuring 2 GB. The matrix driver receives generator PID in its readiness
JSON. In a temporary driver copy, collect `json.loads(line)["pid"]` alongside
readiness, and start `sample <pid> 2 1 -file <unique-path>` immediately after
sending all `GO` lines, before awaiting completion. Use node metrics-on B/C and
also A. Compare generator UDP receive/decrypt/TCP ingress/ACK egress stacks,
per-process CPU, node TCP data sizes, WG/UDP batch sizes and arrival burst shape.
If generator cost/GB remains high with identical packet sizes, look for extra
per-datagram poll/yield/ACK work or a receive batch split rather than increasing
the node window. Run the metrics-off A/B separately.

To reproduce an isolated control without the shared harness:

```sh
cd /Volumes/git/ts-exit/scratch/wt-codex-shard
TS_EXPERIMENT_FORWARDER_SHARDS=4 TS_EXPERIMENT_SHARD_BATCH=1 TS_EXPERIMENT_SPLICE_COALESCE=1 TS_EXPERIMENT_TARGETED_REPLAY=1 TS_EXPERIMENT_RX_BATCH=1 TS_EXPERIMENT_ACK_BATCH=1 TS_EXPERIMENT_NETSTACK_BUDGET=0 TS_EXPERIMENT_PIPELINE_THREAD=0 TS_DEBUG_PIPELINE_METRICS=1 TS_BENCH_SPLICE_WG=1 TS_BENCH_SPLICE_PEER_MODE=harness TS_BENCH_SPLICE_MTU=1280 TS_BENCH_SPLICE_FLOWS=128 TS_BENCH_SPLICE_BYTES=4000000 TS_BENCH_SPLICE_DIRECTION=up cargo +1.95.0 test --locked -p geiserx_ts_runtime --features ring-aead --profile exitnode local_splice_probe -- --ignored --nocapture --test-threads=1
```

Set metrics to `0` for performance, direction to `down` for the download
control, and N=1/flag 0 or N=4/flag 0 for the other variants. Optional
`TS_BENCH_SPLICE_LOAD_ECHO=1` measures once-per-millisecond echo under bulk load;
`TS_BENCH_SPLICE_BUFFER` and `TS_BENCH_SPLICE_MTU` are probe-only controls.
