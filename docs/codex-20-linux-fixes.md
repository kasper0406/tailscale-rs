# Brief 20: Linux RX handoff and splice burst bounds

2026-10-13. Worktree `/Volumes/git/ts-exit/scratch/wt-codex-linux`, branch
`ts-exit/linux-fixes`, base `3e9ee27`. No push or live node restart.
RX batching and splice coalescing still default on only on Apple; `=1` and
`=0` retain their existing meaning. These are Linux mitigation candidates,
not measured Linux performance wins. macOS keeps its measured code paths.

| Commit | Change |
|---|---|
| `f01ba7c` | Bound Linux direct RX handoff to four packets with FIFO/yield/closure tests. |
| `39a6220` | Linux splice direction bounds and immediate snapshots; test both platform policies. |

Both code commits carry the user's signed-off-by and Codex co-author trailers.

## Evidence and root cause

The supplied F28 measurements use kworkstation's Ryzen 9 9950X3D, eight
physical node cores on one CCD (W=8, N=1), generator/sink on the other CCD.
OFF/ALL have two passes; the noisy 2×64 uploads need confirmation.

| Load | OFF Mb/s, CPU-s/GB | RX alone throughput | Coalescing alone throughput |
|---|---|---|---|
| 1×1 down | 4733, 4.62 | 4684, approximately unchanged | 4049, −14%; −18% CPU/GB |
| 1×1 up | 4064, 4.81 | +3%; −19% CPU/GB | approximately unchanged |
| 2×8 down | 3925, 5.06 | −10%; +4% CPU/GB | +4%; −10% CPU/GB |
| 2×8 up | 4647, 5.17 | −18% | −3% |
| 2×64 down | 3347, 6.17 | −8% | +1%; +15% CPU/GB |
| 2×64 up | 4752, 6.21 | −27%, noisy | −12%, noisy |

ALL used roughly two node cores versus three with OFF: efficiency increased
while throughput sometimes fell. macOS results in the supplied brief remain
the priority: coalescing saved 31% CPU/GB on single-flow download; RX batching
saved 17% on 2×64 upload. The unchanged defaults protect those gains.

Code inspection refutes two parts of the proposed explanations:

- `ts_magicsock::receive_batch::Receiver` consumes at most 16 datagrams per
  nonblocking kernel call. Shared/connected pumps retain owned buffers and
  hand off whole batches. `DirectTransport::recv` groups by peer; the optional
  non-kernel bridge drain takes up to 64 ready envelopes. `run_direct` sends
  each peer group to the dataplane.
- `ts_dataplane::async_tokio::DataPlane::step` holds `core_state` while calling
  synchronous `process_inbound_from`. WG decrypt, source/ACL filtering and
  dispatch have one owner **with RX both off and on**. OFF never spawned a
  decrypt task per datagram. Adding tasks around the same mutex cannot create
  parallel decryption; safely changing session/replay/security ownership would
  be a larger design change.
- Netstack admission has a separate pump. It flushes deferred RX wakes at
  each message boundary (and at its 128-packet turn bound), so a larger
  plaintext message can delay the first stack wake. Larger WG messages also
  occupy the serial owner longer before it can service outbound traffic.
- The existing splice drains only immediately ready reads and returns the
  collected bytes on `Pending`, EOF or the 128 KiB capacity. There is no timer
  or wait-to-fill. A fast ready source can nevertheless accumulate a much
  larger burst before publishing it to the opposite writer.

The verified issue is larger uninterrupted work and publication bursts,
not newly serialized WG ownership or a timed coalescing hold. Their causal
contribution to F28 remains a hypothesis pending the real A/B.

For **downloads**, RX primarily receives the peer's ACKs. A group of ACKs
must finish decrypt/filter/dispatch before netstack admission wakes the TCP
driver. Delaying that wake can delay freeing the send window and scheduling
the next download flight, especially across many flows. WG inbound groups
also compete with outbound encryption on the same owner. This explains a
plausible RX-only download regression without claiming lost concurrent
decrypt workers or proving slower congestion-window growth. Full-path ACK
timing/window occupancy would be needed to prove the explanation.

## Changes

**RX, Linux only when `TS_EXPERIMENT_RX_BATCH=1`:** `run_direct` splits each
peer group into owned FIFO messages of at most four packets. It publishes a
chunk, then yields before publishing its remaining tail. A short group goes
straight through, without a yield or a new allocation in the helper. There
is no waiting to fill, detached worker, new queue, packet cloning, session
migration or security bypass. Decrypt remains serial; the smaller messages
allow stack admission and ACK work to overlap with the remaining receive
work and allow other ready tasks to run between chunks.

The four-packet bound is a conservative tuning candidate, not an empirically
optimal value. More messages increase channel, allocation and routing costs;
that may spend some CPU savings to recover throughput. Kernel `recvmmsg`
still receives up to 16 per call, and the existing downstream capacities,
128-packet turn bounds, per-peer packet order and channel-close behavior
remain. Apple and the flag-off path retain whole-group handoff. Shard
placement, DERP receive, nonce/replay handling and ACL/source checks are
unchanged.

**Splice, Linux only when `TS_EXPERIMENT_SPLICE_COALESCE=1`:** retain a
128 KiB overlay→host buffer and `RecvCoalesced`'s one ready netstack snapshot
across ring wrap. Return that snapshot immediately to Tokio's writer rather
than polling another receive to gather more. Host→overlay uses a **16 KiB**
buffer and returns each host read immediately, bounding the next netstack
Send command instead of assembling 128 KiB download bursts. No timer,
fill threshold or manual writable wait is added. Tokio still owns short
writes, backpressure, flush, both independent shutdowns and error behavior.
It may top up buffered data when a writer is Pending; every copy buffer stays
bounded, and the read adapter does not delay already returned data itself.

Apple retains ready-read drains and 128 KiB buffers in both directions.
Flag-off retains Tokio's ordinary 8 KiB buffers. Linux flagged copy storage
falls from 256 KiB to 144 KiB per flow: 112 KiB less, or 224 MiB less at the
2,048-flow cap. In-flight Send payloads also fall from up to 128 KiB to
16 KiB; existing netstack and kernel TCP buffers do not change.

## Counter interpretation

The brief-18 `host_write_sizes` histogram measures actual successful writes
**to the host**: upload data and download request headers. It cannot confirm
the size or latency of the host→overlay download commands. The download
regression therefore cannot be diagnosed from its mean host write size.
No instrumented kworkstation measurements have been made in this worktree.

For a separate metrics-on upload control, take matched-window deltas of
`host_write_sizes`, `host_write_bytes`, `host_write_partial` and
`host_write_pending`. Buckets remain inclusive upper bounds of 1024, 2048,
4096, 8192, 16384, 32768, 65536, 131072, unlimited bytes.

```text
writes = sum(delta(host_write_sizes))
mean bytes/write = delta(host_write_bytes) / writes
writes/GB = writes * 1e9 / delta(host_write_bytes)
```

The Linux upload policy should retain large writes when one ready netstack
snapshot already contains many bytes. Compare actual histograms rather than
assuming that disabling repeated reads preserves all syscall savings. Also
collect `Snapshot.receive.kernel_batches`, `Snapshot.egress_batches`,
`Snapshot.ingress_drops` and device egress queue occupancy. Occupancy snapshots cannot exclude brief
saturation. Keep instrumented controls separate from throughput/CPU results.

## Local Apple probe

Eight optimized brief-18 probe runs passed all payload and EOF checks, with
zero peer ingress drops. Two fresh-process passes reverse coalescing order
(`0/1`, then `1/0`). N=1, four caller workers, one in-memory peer, production
forwarder/netstack/host sockets; no WG/control and no separate sink CCD.
Replay/RX are `1`, ACK batching/budget/pipeline are `0` throughout.
One-flow runs transfer 64,000,000 bytes in each direction, metrics off.
The 16-flow upload runs transfer 8,000,000 bytes per flow, metrics on.

| Observation, range across two runs | Coalescing 0 | Coalescing 1 |
|---|---:|---:|
| One-flow down, Mbit/s | 7318–7387 | 11250–11599 |
| One-flow up, Mbit/s | 2484–2527 | 2549–2598 |
| One-flow echo p50, µs | 33.5–36.2 | 33.5–34.4 |
| One-flow echo p99, µs | 54.0–77.9 | 73.8–84.7 |
| 16-flow up, actual mean host write, bytes | 6778–6857 | 16050–16121 |

All four instrumented uploads had zero host-write Pending/partial polls.
The Apple flag retains the large-write and download-rate benefits in this
fixture. The echo tail ranges overlap; two passes cannot establish tail
neutrality. This is the new binary's flag-off/on comparison, not a measured
before/after comparison with the base binary. Short local transfers, shared
CPU and the missing WG path prevent using these rates as node CPU/GB or
Linux evidence. The Linux four-packet handoff is not active on Apple, and
the Linux copy policy is exercised by the semantic tests, not these probes.

Reproduce on Apple, substituting `COAL=0/1`, `FLOWS=1`, `BYTES=64000000`,
`METRICS=0`; for the instrumented control use `FLOWS=16`, `BYTES=8000000`,
`METRICS=1` and also set `TS_BENCH_SPLICE_DIRECTION=up`:

```sh
TS_EXPERIMENT_TARGETED_REPLAY=1 TS_EXPERIMENT_RX_BATCH=1 \
TS_EXPERIMENT_ACK_BATCH=0 TS_EXPERIMENT_FORWARDER_SHARDS=1 \
TS_EXPERIMENT_NETSTACK_BUDGET=0 TS_EXPERIMENT_PIPELINE_THREAD=0 \
TS_EXPERIMENT_SPLICE_COALESCE="$COAL" TS_DEBUG_PIPELINE_METRICS="$METRICS" \
TS_BENCH_SPLICE_FLOWS="$FLOWS" TS_BENCH_SPLICE_BYTES="$BYTES" \
cargo +1.95.0 test --locked --profile exitnode -p geiserx_ts_runtime \
  --features ring-aead --lib local_splice_probe \
  -- --ignored --nocapture --test-threads 1
```

The actual runs invoke that Cargo-built optimized test executable directly,
with the same settings, after all builds and suite runs finish. Raw logs:
`/tmp/codex-20-probe-f{1,16}-c{0,1}-r{1,2}.log` and
`/tmp/codex-20-probes.log`.

## Exact Claude A/B

Build one binary, record its source commit and SHA-256, and reuse it for all
four variants:

```sh
cargo +1.95.0 build --locked --profile exitnode --example exit_node \
  --features ring-aead
```

Keep **W=8, N=1**, eight physical node cores on one CCD, generator/sink on the
other CCD, TCP buffers **262144**, the same authenticated peers, direct path,
sink, fixture refresh/warmup and settled external hold sessions. Keep node
launch/control arguments identical. The generator keeps `--workers 2`.

Explicitly fix these settings for the attribution matrix:

```text
TS_EXPERIMENT_FORWARDER_SHARDS=1
TS_EXPERIMENT_TARGETED_REPLAY=0
TS_EXPERIMENT_ACK_BATCH=0
TS_EXPERIMENT_NETSTACK_BUDGET=0
TS_EXPERIMENT_PIPELINE_THREAD=0
TS_DEBUG_PIPELINE_METRICS=0
TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=0
TS_DEBUG_MAGICSOCK_BATCHED_IO=0
TS_DEBUG_DONT_FRAGMENT=0
```

Replay is forced off to isolate RX/coalescing and match the supplied OFF
control. Linux ignores the Apple-only connected-socket/send-batch/DF knobs
anyway; do not silently change those between variants.

| Variant | `TS_EXPERIMENT_RX_BATCH` | `TS_EXPERIMENT_SPLICE_COALESCE` |
|---|---:|---:|
| OFF | 0 | 0 |
| fixed RX | 1 | 0 |
| fixed COAL | 0 | 1 |
| both | 1 | 1 |

| Load | Peers × flows each | Bytes per flow | Directions | Passes |
|---|---|---:|---|---:|
| p1f1 | 1 × 1 | 2,000,000,000 | down, up | 2 |
| p2f8 | 2 × 8 | 250,000,000 | down, up | 2 |
| p2f64 | 2 × 64 | 30,000,000 | down, up | 2 |

**24 cells, 48 transfers**, or 24 harness invocations that each run both
directions once. For each load use OFF/RX/COAL/both in pass 1 and
both/COAL/RX/OFF in pass 2. Restart with the selected variant's environment,
refresh fixtures and settle holds before each invocation. Do not choose
favorable runs or discard slow/stalled flows.

Use the workspace harness containing `--external-hold` (the older harness
inside this base worktree lacks that option). Example p2f8 invocation, with
the same process/fixture variables used for F28:

```sh
taskset -c "$GEN_CPUSET" python3 "$HARNESS_ROOT/runner/synthetic-matrix.py" \
  --binary "$SYNTHETIC_PEER_BIN" --peers "$PEERS_FILE" \
  --node-pid "$NODE_PID" --node-binary "$NODE_BIN" \
  --label "linux20-${VARIANT}-pass${PASS}-p2f8" \
  --flows-per-peer 8 --bytes 250000000 --workers 2 --runs 1 \
  --timeout 120 --external-hold
```

Set `HARNESS_ROOT` to the existing kworkstation workspace with the current
F28 harness. Use a one-peer fixture for p1f1 and two-peer fixtures for the other loads;
substitute the table's flow/byte values. Pin the node with the existing
launcher and `--worker-threads 8 --tcp-buffer-size 262144`. Choose CPU sets
from the machine's physical-core/CCD topology; do not assume SMT sibling
numbers. Record both individual passes, throughput, node CPU-s/GB,
generator/sink CPU, peak RSS, drops, completion counts, stalls and tails.
Two passes establish direction and spread, not statistical certainty.

Add four **separate** metrics-on p2f8-up controls (one per variant) for write
histograms; use metrics-off for all 48 main transfers. If single-flow down
still loses, compare full-path TCP echo while idle and under load, ACK/window
timing and CPU scheduling profiles before increasing buffers or adding tasks.

Do not enable either default globally from these code changes alone. A
default-on proposal requires matched Linux results without the F28 losses
and a macOS check confirming the existing gains, including single-flow
download and many-flow upload. After attribution passes, a small follow-up
with targeted replay restored to `1` checks the shipped combination.

## Validation

Validation runs on this Apple host, using `cargo +1.95.0` and `--locked`.
Linux policies are explicitly injected into the splice unit tests here;
the RX helper is tested with both a four-packet bound and unlimited handoff.
Those tests do not require a Linux syscall or change the production flags.

New coverage checks counts 0/1/4/5/16/64/65, both peers' FIFO order, exact
tails, no empty tail messages, a consumer running before the producer's
second chunk on one worker, and immediate stop when the receiver closes.
Splice coverage checks returning each snapshot without polling the next
ready read, `Pending`/error behavior, 128/16 KiB direction limits and
write-before-next-snapshot on a ready writer. The existing backpressure and
both-FIN-orders test now runs OFF, Apple and Linux policies explicitly,
including the reply that starts after the initiating FIN. Existing tests
cover real netstack ring wrap, receive cancellation/remainders, large
lossless tails, source/ACL changes, proxy fail-closed behavior and runtime
shard fairness/admission/shutdown.

The package suite is the same eight-package unit/integration/doctest suite
used in briefs 15–19:

```sh
cargo +1.95.0 test --locked \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime -p geiserx_ts_magicsock \
  -p geiserx_ts_tunnel -p geiserx_ts_dataplane \
  --features geiserx_ts_runtime/ring-aead
```

Here “common on” means replay/RX/ACK/metrics/connected sockets/batched IO/DF
are `1`, with their actual knob names as given in the A/B settings above.
Non-selected knobs are unset. The first launcher pass mistakenly used old
names for connected sockets, DF and pipeline placement; the final matrix
reruns all ten modes with the correct names. Only final logs/counts are
used for the results.

| Mode | Shards | Common knobs | Coalescing | Budget / pipeline |
|---|---:|---|---|---|
| default | unset | unset | unset | unset |
| off | 1 | all `0` | 0 | both `0` |
| flags | unset | on | 0 | unset |
| n1 | 1 | on | 0 | unset |
| n4 | 4 | on | 0 | unset |
| coal | unset | unset | 1 | unset |
| on | 1 | on | 1 | unset |
| coal-n4 | 4 | on | 1 | unset |
| budget | 4 | on | 1 | budget `1` |
| pipeline | 1 | on | 1 | pipeline `1` |

All ten final modes pass **1,113 tests in 29 groups per mode**, with zero
failures. The one ignored test is the local measurement probe, invoked
separately below. This includes the default/unset, forced-on and forced-off
RX/coalescing controls.

Forwarding integration tests pass three additional runs for each of
N=1/4 × coalescing=0/1, common knobs on, budget/pipeline off: **17 tests
per run, 12 runs**, zero failures. The library harness does not consume the
runtime shard-placement knob; the full runtime suite provides actual N=4
ownership/placement coverage. Logs: `/tmp/codex-20-forwarding-n*-c*-r*.log`
and `/tmp/codex-20-forwarding.log`.

Affected libraries and the exit-node example pass Clippy with `-D warnings`. An additional
`--all-targets` Clippy run hits the pre-existing `async_yields_async` lint
in `ts_forwarder/src/shard_resources.rs:140`, where the panic test returns
a spawned JoinHandle from its task-local scope. No lint suppression or
unrelated source edit was added. Core no-default-features and socket
futures-io checks pass. Touched-file nightly rustfmt and `git diff --check`
pass.

Linux forwarder and magicsock **library and test code**, and Linux Tokio
netstack libraries, cross-check with the existing verified 1.95.0 sysroot
at `/tmp/codex-15-linux-sysroot`. The full Linux runtime cross-check cannot
build `ring`: no `x86_64-linux-gnu-gcc` is installed, and the available LLVM
cross compiler lacks Linux C headers (`assert.h`). No fake C headers or
host-ABI substitution were used. Linux execution/performance testing stays
with kworkstation. The changed direct helper and both policy branches are
type-checked and exercised on the Apple host.

The optimized exit-node build passes. The Apple binary built from code head
`39a6220` is
`/Volumes/git/ts-exit/scratch/wt-codex-shard/target/exitnode/examples/exit_node`
(the existing target cache was reused), SHA-256:

```text
03493aa753815c9f0d322df8e562485ff00b514b31b86827b17a8150d6c9b6de
```

Claude must build and hash the Linux binary locally; this Apple hash is
validation provenance, not a portable binary.

Final logs: `/tmp/codex-20-all-*.log`,
`/tmp/codex-20-validation.log`, `/tmp/codex-20-suite-summary.json`,
`/tmp/codex-20-{rx,coal}-focused.log`, `/tmp/codex-20-clippy-libs.log`,
`/tmp/codex-20-clippy-example.log`, `/tmp/codex-20-clippy.log` (the extra
all-targets attempt), `/tmp/codex-20-{core,socket}-check.log`,
`/tmp/codex-20-linux-{check,netstack,runtime,runtime-clang}.log`,
`/tmp/codex-20-build.log` and `/tmp/codex-20-probe-build.log`.

`bd prime` is unavailable (`command not found`); no tracker sync occurred.
