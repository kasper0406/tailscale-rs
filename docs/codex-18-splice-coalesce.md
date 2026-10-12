# Brief 18: bounded splice coalescing

2026-10-13. Branch `ts-exit/shard-stage1`, base `c1acc85`, worktree
`/Volumes/git/ts-exit/scratch/wt-codex-shard`. Code head `9cc1f29`; the final
commit adds this report only. No push or live node restart.
The new throughput matrix is for Claude; the measurements here isolate the
production forwarder locally, without WireGuard or control sessions.

## Commits

All commits carry the user's signed-off-by and Codex co-author trailers.

| Commit | Change |
|---|---|
| `c7451a8` | Bounded ready TCP receive across ring wrap, socket opt-in and replay/cancellation regression. |
| `85a2b25` | Flagged splice, actual host-write metrics, status logging and lossless/backpressure tests. |
| `d8019bf` | Reuse the shard ingress revalidation Vec. |
| `9cc1f29` | Explicit local write-size/RTT diagnostic probe. |

## Implementation

`TS_EXPERIMENT_SPLICE_COALESCE=1` is cached once, on first splice. Unset or any
other value retains Tokio's existing `copy_bidirectional` and 8 KiB buffers.
The flag is independent of shard count, replay, RX/ACK batching, full budgeting,
and pipeline placement.

Flagged splices use `copy_bidirectional_with_sizes`, with **128 KiB per
direction**, and an `AsyncRead` adapter that gathers successive ready reads
straight into those buffers. It returns the collected bytes immediately when
the next read returns `Pending`, reaches the bound, or observes EOF. There is
no timer, fill threshold, yield, or extra task in the splice. The host-to-overlay
side also drains ready host reads; its larger netstack sends can amortize
command traffic on downloads.

A splice explicitly enables `TcpStream::set_coalesced_reads`. Its subsequent
AsyncRead requests use `RecvCoalesced`: the stack copies up to the caller's
capacity from **all currently queued RX bytes, including ring wrap**, in one
command. Ordinary `Recv` exposes only the first contiguous ring span. The
snapshot is taken when the stack serves the request, without waiting for the
requested length. Application sockets and explicit `recv` APIs retain ordinary
Recv behavior. An empty receive retains the existing blocked-command path.
The new command is classified as a receive for abandoned-request protection
(F17) and as TCP RX for targeted replay; it requires no safety-net wake.

Pending receives remain owned by the stream. A smaller subsequent read buffer
uses its existing remainder handling. Tokio still owns both transfer states:
flush the received tail, shut down only the opposite writer on EOF, continue
the other direction, finish after both shutdowns. Read/write/flush/shutdown
errors terminate the copy immediately, with Tokio's existing possibility of
losing buffered bytes on error. Short writes and write backpressure remain
Tokio's responsibility. No new queue or detached copy worker was added.

Memory: two copy buffers rise from 16 KiB to 256 KiB per active splice, an
additional **240 KiB per flow**, or **480 MiB at the 2,048-flow cap**. In-flight
receive responses and overlay send payloads can each additionally hold up to
128 KiB rather than 8 KiB; their worst simultaneous increase is another
240 KiB per flow. Thus copy/request staging can increase by roughly **960 MiB
at the cap**, excluding existing TCP/kernel buffers, socket generations and
allocator overhead. Allocation stays bounded and admission caps stay shared.
This is a throughput/memory tradeoff to measure, not a default-buffer change.

Separately, shard ingress revalidation reuses its one-packet Vec instead of
allocating/freeing it for every packet. Admission checks, locking, packet
order, queue accounting, barriers and the 128-packet turn bound are unchanged.
This removes an allocation visible directly in the previous pump code; there
is no separate measured speed claim for that cleanup.

## Counters and upload diagnosis

`TS_DEBUG_PIPELINE_METRICS=1` adds `Snapshot.splice` to the runtime snapshot
and the exit-node status log. The log also now prints the existing
`forwarder_shards` snapshots; previously they were available through the API
but omitted by this example.

`host_write_sizes` counts **successful, nonzero host TCP AsyncWrite
completions**, using inclusive upper byte bounds:

```text
1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, unlimited
```

`host_write_bytes` counts accepted bytes, not offered bytes. `host_write_partial`
counts successful short writes. `host_write_pending` counts Pending polls,
which are readiness/backpressure observations, not syscalls. Failed and zero
writes are excluded from byte/size counters. For the production Tokio host
TcpStream, a successful completion corresponds to a socket write. Counters
are process-wide across all forwarder stacks, including N=1. Disabled splices
use a cached bool and perform no atomic updates or hot-path formatting.

Use matched-window deltas:

```text
writes = sum(delta(host_write_sizes))
mean bytes/write = delta(host_write_bytes) / writes
writes/GB = writes * 1e9 / delta(host_write_bytes)
```

Read histogram deltas rather than subtracting bucket means. Down runs mainly
write tiny request headers to the host; use the **up** window to test the
upload hypothesis. Keep metrics-on diagnosis separate from metrics-off
throughput/CPU results because shared atomic counters can perturb N=4.

The supplied call graphs identify the expensive syscall, rather than just
its name. Parsing each `__sendto` leaf under its immediate caller gives:

| Profile | Host `TcpStream::poll_write` samples | Underlay UDP samples | Total |
|---|---:|---:|---:|
| `scratch/prof/replay-up-2x64.txt` | 2,631 | 13 | 2,644 |
| `scratch/prof/shard4-up-2x64.txt` | 3,938 | 233 | 4,171 |

Thus **94.4% of the shard profile's sendto samples are host TCP writes**.
The profile is consistent with smaller host writes, not sufficient to measure
sizes: samples are time spent, not call counts. The new counters provide the
missing direct observation.

The serial WG owner, per-packet keyed dispatch/queue operations, cross-thread
notifications and bounded 64-batch WG handoff remain. The local probe bypasses
that WG handoff and cannot establish whether it saturates in Claude's run.
Compare `egress_batches[1]`, underlay offered/completed batch histograms,
per-shard dispatcher occupancy/drops and device `egress_queued` over the same
window. Sustained device egress occupancy suggests downstream pressure; it
does not prove a full WG mailbox. Empty one-second snapshots cannot exclude
short saturation or wake storms. No capacity increase or security-barrier
shortcut is justified by the supplied profile.

## Local measurement procedure

The ignored `forwarder_shards::tests::splice_probe::local_splice_probe` uses
production N=1/N>1 forwarder startup, source/ACL state, shard dispatch, gates,
netstack sockets, host TCP sockets and splice placement. It uses a bounded
in-memory peer, 4 caller workers, 256 KiB TCP buffers and a loopback sink.
It warms a TCP connection for 100 echoes, then times 1,000 sequential 64-byte
echoes. Transfers verify every payload byte and both EOFs. The receive ring
can drop under overload; the probe reports drops and allows TCP retransmission.
The local sink/generator share its process/runtime. Rates are diagnostics,
not node-only CPU/GB or the two-authenticated-peer benchmark.

```sh
TS_EXPERIMENT_TARGETED_REPLAY=1 TS_EXPERIMENT_RX_BATCH=1 \
TS_EXPERIMENT_ACK_BATCH=1 TS_DEBUG_PIPELINE_METRICS=1 \
TS_EXPERIMENT_FORWARDER_SHARDS=4 TS_EXPERIMENT_SPLICE_COALESCE=1 \
TS_BENCH_SPLICE_FLOWS=128 TS_BENCH_SPLICE_BYTES=4000000 \
TS_BENCH_SPLICE_DIRECTION=up \
cargo +1.95.0 test --locked --profile exitnode -p geiserx_ts_runtime \
  --features ring-aead --lib local_splice_probe -- --ignored --nocapture
```

Run alone, with full budgeting/pipeline mode off. Change N and coalescing for
the four diagnostic controls; omit DIRECTION to verify both directions. For
single-flow diagnostics use `FLOWS=1`, `BYTES=64000000`. Fresh startup chooses
a fresh keyed owner; do not select favorable distributions. The flags-off
current binary provides counter instrumentation absent from `c1acc85`.

## Local results and the latency claim

Three interleaved fresh-start repeats per setting; 24 completed probe runs.
For writes, one peer × 128 flows × 4,000,000 bytes, metrics on; for idle echo
and single-flow transfers, metrics off. All payload/EOF checks passed.

| N | Coalesce | Median mean host write, bytes (range across runs) | Writes >8 KiB, pooled fraction | Peer ingress drops, sum of three runs |
|---:|---:|---:|---:|---:|
| 1 | 0 | 4,025 (3,592–5,183) | 0% | 2,888 |
| 1 | 1 | 5,551 (5,529–5,726) | 16.3% | 0 |
| 4 | 0 | 6,055 (5,409–6,280) | 0% | 16,653 |
| 4 | 1 | 12,291 (11,161–12,701) | 51.5% | 20,162 |

The flag increases median mean write size **1.38× at N=1 and 2.03× at N=4**.
The counter **does not confirm smaller N=4 baseline writes**: they were
1.50× larger than N=1 here. Thus the simple smaller-write explanation is
refuted for this isolated fixture, while the original end-to-end hypothesis
remains unconfirmed. The profile establishes the host syscall hotspot, not
its cause. The peer ring's ACK drops, shared generator/sink CPU, different
burst shape and absent WG path prevent treating these rates as a production
win. Instrument the real WG workload before attributing its extra CPU/GB to
write count rather than per-write cost or contention.

Host partial/Pending counts were small: N=4 flag=0 had two partial writes and
three Pending polls total; N=4 flag=1 had eight and thirty respectively.
N=1 had neither. This excludes persistent host-write backpressure **in this
probe**, not in the original WG workload.

| N | Coalesce | Median echo p50, µs (range of run p50s) | Median echo p99, µs | Local single-flow download, median Mbit/s |
|---:|---:|---:|---:|---:|
| 1 | 0 | 33.5 (32.4–33.7) | 79.7 | 6,762 |
| 1 | 1 | 33.5 (33.5–33.8) | 81.2 | 12,385 |
| 4 | 0 | 38.8 (38.6–39.2) | 85.4 | 8,481 |
| 4 | 1 | 39.3 (39.1–39.8) | 87.8 | 11,045 |

There is a reproducible **5.3 µs local median RTT increment** from N=1 to N=4
with coalescing off. Coalescing leaves the median essentially unchanged.
Single-flow probes had zero peer ingress drops. This verifies a small net
placement/dispatch latency cost; it does **not** reproduce the supplied
single-flow download regression (N=4 without coalescing downloads faster in
this fixture). Coalescing helps local download, but these numbers exclude WG.

As a diagnostic calculation only, a completely window-limited 256 KiB flow
at the supplied 4,300 versus 3,307 Mbit/s would need effective window turnover
of roughly **488 versus 634 µs**, an increment of **146 µs**. The idle local
5.3 µs increment cannot explain that alone. Measure full-path idle/load echo
and window occupancy/ACK timing before calling the original loss a verified
latency/window effect. Cross-thread WG feedback, egress handoff, scheduler
contention under load or burst shape can add costs absent from this probe;
these are hypotheses, not measured conclusions.

Raw logs are `/tmp/codex-18-probe-n{1,4}-c{0,1}-f{1,128}-{1,2,3}.log`, with
parsed rows in `/tmp/codex-18-probes.json` and summaries in
`/tmp/codex-18-probes.log`. Echo owner indices are recorded per startup.

## Next Claude A/B: three variants, six workloads

Build `c1acc85` and the final code head with:

```sh
cargo +1.95.0 build --locked --profile exitnode --example exit_node \
  --features ring-aead
```

Record commit and binary SHA-256. Keep the §4 harness, peer identities,
endpoints, loopback sink, direct path, four node workers and **262144-byte TCP
buffers** fixed. Keep targeted replay, RX and ACK batching, connected sockets,
batched underlay IO and DF on. Full budgeting and pipeline mode stay off.
Use **metrics off** for the required performance matrix.

| Variant | Binary | Shards | Splice coalescing |
|---|---|---:|---:|
| prev-N=1 | `c1acc85` | 1 | unset |
| coalesce-N=1 | new | 1 | 1 |
| coalesce-N=4 | new | 4 | 1 |

| Load | Peers × flows per peer | Bytes per flow | Directions | Repeats per variant |
|---|---|---:|---|---:|
| `p1f1` | 1 × 1 | 2,000,000,000 | down, up separately | 3 |
| `p2f8` | 2 × 8 | 250,000,000 | down, up separately | 3 |
| `p2f64` | 2 × 64 | 30,000,000 | down, up separately | 3 |

**18 cells / 54 runs total.** For each load, interleave variant orders
prev/one/four, four/one/prev, one/prev/four over the three repeats. Each
invocation uses `synthetic-matrix.py --runs 1 --external-hold`, with existing
`--workers 2 --timeout 120`, down then up. Keep peer hold sessions alive and
settled, refresh disco fixtures after startup, and retain all flow completion
and EOF checks. Report median/spread Mbit/s, node CPU-s/GB, generator/sink
CPU and peak RSS; retain errors/stalls, duration distributions and tails.

For the counter diagnosis, use **the same new binary** in four small controls
(N=1/4 × flag=0/1), only `p2f64 up`, metrics on and status cadence 1 second.
Three repeats give 12 diagnostic transfers; do not fold them into the
performance matrix. New N=4 flag=0 includes the isolated Vec reuse cleanup.
Collect splice histogram/byte deltas and shard/egress snapshots. This distinguishes
whether coalescing restores write size from improvements elsewhere.

Include idle TCP echo per variant and one separate N=1/N=4 load-plus-echo
comparison, using settled TCP sessions. `runner/rttprobe.py` measures TCP sink
echo; ICMP echo is forced to shard 0 and has no host splice. For a remaining
single-flow loss, one diagnostic N=4 run at a larger TCP window can test the
window ceiling, without changing the required matrix. Avoid transferring a
flow between shards: ownership/generations are fixed for its lifetime.

The cheapest next stage-1 mitigation is grouping the already authenticated
WG ingress batch by owner and waking each shard once, flushing at the end of
the available batch. Preserve per-flow ordering, per-packet/byte capacity and
ordered security barriers; never wait to fill. Both directions already drain
available egress into batches of up to 64, so merely increasing mailbox depth
is not this mitigation. A dominant-flow bypass/migration would need a new
ownership and supervision design; it is not part of this change or stage 2/3.

## Validation

The brief-15/17 package suite passed **1,109 tests in 29 groups per mode**,
with no failures. The one ignored test is the explicitly invoked diagnostic
probe, which passed all 24 measured runs.

| Mode | Shards | Replay/RX/ACK/metrics/connected/batched/DF | Coalesce | Full budget / pipeline |
|---|---:|---|---:|---|
| default | unset | unset | unset | unset |
| flags | unset | all on | unset | unset |
| explicit one | 1 | all on | unset | unset |
| shards | 4 | all on | unset | unset |
| coalesce alone | unset | unset | 1 | unset |
| coalesced one | 1 | all on | 1 | unset |
| coalesced shards | 4 | all on | 1 | unset |
| budget control | 4 | all on | 1 | budget=1 |
| pipeline control | 1 | all on | 1 | pipeline=1 |

```sh
cargo +1.95.0 test --locked \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime -p geiserx_ts_magicsock \
  -p geiserx_ts_tunnel -p geiserx_ts_dataplane \
  --features geiserx_ts_runtime/ring-aead
```

New regressions cover ready-data/Pending flushing, bounded reads and tails,
immediate read errors, actual short-write counter accounting, 517/1031-byte
duplex backpressure with both FIN orders, a 512 KiB+37-byte patterned transfer
and reply after EOF through real netstacks/host TCP, plus RX ring wrap, receive
limits, cancellation/F17 and FIN on targeted replay without a safety sweep.
Existing four-shard tests cover cold TCP/UDP, both half-close orders on every
shard, shared admission, failure, security updates and shutdown.

Forwarding integration tests passed **three additional runs for each of
N=1/4 × coalesce=0/1**, 17 tests per run, with the other flags on. The library
forwarder harness does not consume shard-count placement; the runtime suite
and probe provide actual N=4 coverage. A test-only unused-result warning was
corrected after the full matrix; the focused counter test and all extra
forwarding runs passed afterward.

The optimized exit-node build succeeded. Binary:
`target/exitnode/examples/exit_node`, SHA-256
`5aee13d03651525128bbd1e1cb45ee93142ac7db1fe5089daba9f990f2c3f45a`.
Its source content matches code head `9cc1f29`. Build log:
`/tmp/codex-18-build.log`.

Affected libraries and the exit-node example passed Clippy with `-D warnings`.
The core no-default-features check, socket futures-io check and Linux netstack
library cross-check with Tokio passed. Linux used the verified 1.95.0 sysroot
at `/tmp/codex-15-linux-sysroot`; no Linux runtime or benchmark was executed.
Touched-file nightly rustfmt and `git diff --check` passed. Cargo commands used
`+1.95.0` and `--locked`.

Logs: `/tmp/codex-18-all-*.log`, `/tmp/codex-18-forwarding-*.log`,
`/tmp/codex-18-clippy.log`, `/tmp/codex-18-check-*.log`,
`/tmp/codex-18-probe-build-final.log`. `bd prime` is unavailable
(`command not found`); no tracker sync occurred.
