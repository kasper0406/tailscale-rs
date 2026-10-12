# Brief 16: separate batching from full work budgeting

2026-10-13. Worktree `scratch/wt-codex-scans`, branch
`ts-exit/netstack-scans`, based on `77e0f36`. The brief-15 ownership/reuse and
trace-level changes remain in every mode. Commits carry the existing
Signed-off-by and GPT-6.1 Co-Authored-By trailers. No push.
Implementation commit: `31b0668`.

**The full budget cuts single-flow egress short after one segment, even with
127 unused transmit tokens.** This is reproduced in a packet-level TCP test.
RX batching now limits receive work without cutting egress short. Performance
and completion-time fairness still need Claude's A/B; no throughput claim is
made from the tests.

## Controls and bounds

Only the exact value `1` enables an experiment. Set flags before startup.

| Flag | Behavior in the ambient async runtime |
|---|---|
| `TS_EXPERIMENT_ACK_BATCH=1` | Publish device egress in chunks of at most 64; flush the partial chunk before the I/O poll returns. No full driver budget or egress-pump yield. |
| `TS_EXPERIMENT_RX_BATCH=1` | Receive at most 16 datagrams per syscall, retain direct receive batches and defer admission wakes. Keep the receive-side bounds below. No transmit-token budget or egress-pump yield. |
| `TS_EXPERIMENT_NETSTACK_BUDGET=1` | Enable the previous full 128-item ingress/egress/command budget, one ingress/egress pass per I/O poll, a driver yield after every turn, admission yields every 128 packets, and an egress-pump yield every four batches. Independent of ACK/RX flags. |
| `TS_EXPERIMENT_PIPELINE_THREAD=1` | Keep the previous full budget and pump yields regardless of the new budget flag. Existing pipeline receive coalescing and dataplane placement remain. |

ACK and RX may be measured separately from **full** budgeting. RX retains a
smaller *scope* of budgeting for executor fairness, rather than becoming wholly
unbounded:

- Native receives and demux handle one batch of at most 16 at a time. They do
  not wait for a full batch or synchronously drain the kernel until empty.
- Connected receive pumps and the direct demux pump retain their yield every
  four RX batch turns (at most 64 datagrams). This includes control-only demux
  turns. The direct bridge takes one native batch, not an unbounded inbox drain.
- Admission yields every 128 packets, including within a larger channel
  message, and publishes its deferred receive wake **before** yielding.
- The netstack itself consumes at most 128 input packets in one async poll,
  then lets egress drain until idle or blocked. It yields when the input bound
  is reached; short receives, empty polls and ordinary command turns do not
  introduce an unconditional yield. Commands get service before each new
  receive-bounded turn. Dirty receives and turn-end output publication still
  complete before the poll returns.

Bounding only the receive syscall is insufficient: an async function can keep
completing synchronously, and the flume device pipe can be refilled by another
worker while the driver drains it. The 4,096-packet ingress ring and its
MTU-based byte cap bound memory but do not bound CPU time under concurrent
refill. The receive-only driver bound closes that gap. With input paused during the egress drain, TCP
cannot continuously reopen its send window from arriving ACKs; queued socket
buffers and pending operations provide finite output work.

The 128-packet receive limit retains the earlier receive chunk size (eight full
native batches); it is a narrow scope change, not a claim that 128 is the
experimentally optimal threshold. It does not limit a single download's
transmit tokens or number of egress passes. The full budget takes precedence
when both are configured, so the new flag plus the old batching flags retains
the brief-15 budgeted behavior, including in pipeline mode.

## Why the old path is expensive

`smoltcp::Interface::socket_egress` visits each socket once per pass.
`tcp::Socket::dispatch` emits **one TCP segment** per visit. Unbudgeted
`IoPoller` repeats egress while it progresses. The full budget's unconditional
`if budgeted { break; }` stops after the first pass, independently of whether
the `TxBudget` actually ran out.

Thus a single ready TCP download socket can emit one segment, finish/flush the
turn, return Ready, yield the driver, calculate the next `poll_delay` by walking
sockets, and start again. It gets no 128-packet amortization. The runtime
egress pump can observe these singleton publications. ACK publication batching
cannot enlarge an output turn that contains only one data segment. On Apple,
`send_wireguard_batch` skips `sendmsg_x` for a singleton peer batch and uses
ordinary UDP sends instead.

The TCP regression test compares the two bounds on an established connection
with 8,000 bytes queued: full budgeting emits exactly one packet, records one
egress pass and zero transmit-budget hits, and requests a yield. Receive-only
budgeting emits multiple packets in the same poll and requests no yield when
there was no inbound work. A separate five-packet test proves the receive bound
survives the egress loop and that a final short drain does not yield.

The cheap fix here is to remove full budgeting from the batching flags and keep
only the receive-side protection for RX. The old full-budget algorithm is
deliberately retained behind its own flag as an attribution control and to
preserve pipeline behavior. Making that algorithm drain 128 *total* transmits
across repeated socket passes, rather than just allowing one pass, would be a
separate change involving turn accounting and socket scheduling. The ~65%
CPU/GB magnitude is not established by the code inspection alone.

## Download fairness investigation

The supplied `bench/results/stage0/batch-{base,rx}/p2f64.jsonl` files show
within-peer dispersion as well as occasional peer imbalance:

| Mode/run | Peer 0 duration min/median/max (s) | Peer 1 duration min/median/max (s) |
|---|---|---|
| base/1 | 5.451 / 5.493 / 5.517 | 5.457 / 5.496 / 5.518 |
| base/2 | 4.601 / 5.182 / 5.523 | 5.199 / 5.282 / 6.247 |
| base/3 | 5.426 / 5.463 / 5.590 | 5.441 / 5.483 / 5.594 |
| RX/1 | 3.646 / 5.673 / 6.325 | 3.821 / 5.677 / 6.426 |
| RX/2 | 1.920 / 5.580 / 6.181 | 5.904 / 6.443 / 7.643 |
| RX/3 | 5.079 / 6.541 / 7.297 | 5.335 / 6.692 / 8.151 |

Every peer's flow-start spread is below 0.3 ms, so start staggering does not
explain seconds of spread. RX's fastest flow IDs also vary across runs: peer 0
starts with 58/21/11 in run 1, 34/39/18 in run 2, and 55/2/48 in run 3. Flow ID
does not establish smoltcp socket insertion order. The evidence does not support
attributing the entire effect to one consistently starved connected socket.

The receive path has no explicit per-flow scheduler. Connected pumps put batches
into a common FIFO; the shared-socket and connected-channel sources compete in
an unbiased Tokio select. That randomizes sources, not individual peers in the
connected FIFO. A native batch can contain a burst of ACKs for one peer or a
subset of its flows. Grouping uses a HashMap, preserves FIFO within a peer, and
does not repeatedly drain one peer until empty. Neither grouping nor the
16-packet native bound alone guarantees equal TCP service.

There is also a fixed-order egress risk: smoltcp restarts its socket walk at the
beginning, and a truly exhausted transmit budget breaks the walk before later
sockets. With exactly 128 transmitting TCP sockets emitting one segment each,
the 128-token limit covers all of them; it is **not sufficient evidence** of
prefix starvation in this 2×64 test. Additional emitting sockets can change
that. The unconditional one-pass return still couples incoming ACK bursts to
small output turns. Receive-only mode removes both the transmit cutoff and
that egress turn restriction, though RX's ACK grouping can still affect TCP
feedback timing.

First compare RX without full budgeting to RX with it. If RX-only remains
unequal, retain per-peer and per-flow completion distributions, reverse peer
launch order, and compare 1×128 with 2×64. Capture ACK/data timing, window
advertisements, retransmissions and loss by peer and TCP 5-tuple in an
attribution run. Global counters cannot identify which flow lost admission or
prove peer starvation. A per-peer scheduling rewrite is not justified by the
current evidence.

## Counters to confirm the mechanism

Use `TS_DEBUG_PIPELINE_METRICS=1` in separate attribution runs; keep it off for
CPU/GB A/B. The existing exit-node status log prints cumulative snapshots
(`--status-interval` must be nonzero). Subtract snapshots over matched transfer
windows; counters are process-wide, and core counters combine both netstacks.

| Counter | Readout |
|---|---|
| `egress_batches[1][0..64]` | Forwarder publication-to-dataplane distribution. Calculate packets `P = sum(i * delta[bucket i])`, calls `C = sum(delta[bucket i])`, mean `P/C`, and singleton share `delta[bucket 1]/C`. A budget-induced collapse should be visible on single-flow download. |
| `sendmsg_x_offered_batches`, `sendmsg_x_batches` | Compare offered and completed peer batch sizes. Singleton peer sends bypass this syscall and are absent, so these histograms alone are not an all-UDP-send denominator. Mixed-peer routing can fragment a larger forwarder batch. |
| `netstack.io_polls`, `netstack.io_ready` (new) | I/O future polls and Ready returns. Compare per output packet/GB. Full budgeting yields after every driver turn, including command turns; Ready returns measure its I/O portion, not every scheduler wake or yield. |
| `netstack.egress_passes` (new) | Transmit-ready smoltcp passes, including idle passes. Full-budget single-flow turns should show many passes/GB but about one pass per I/O poll; receive-only output can use multiple passes in a poll. |
| `netstack.egress_budget_hits` (new) | Passes using all transmit tokens. Few/zero hits alongside many small turns confirms premature one-pass returns rather than actual 128-token exhaustion. Nonzero hits are a reason to investigate fixed-order socket service. |
| `netstack.ingress_budget_hits` (new) | Receive drains reaching their limit, including a drain ending exactly at the limit. RX-only should yield on these hits, not on every transmitted segment. |
| `receive.kernel_batches[0..16]`, `fallbacks`, `truncated` | Confirm RX batching is active, how full receives are, and whether native receive falls back or truncates. Ordinary fallback packets are not counted in the native histogram. |
| `ingress_drops[1]`, replay/dirty/safety counters | Check admission loss and replay work per GB. Safety-net completions suggest missed wakes rather than a batching win. |

Reprofile ordinary UDP sends, scheduler/wake work and socket/timer scans
separately from host TCP writes. The brief-15 `__sendto` attribution correction
still applies: host writes to the sink are a different call path.

## Claude's next flag matrix

Run the **same new binary** for all rows in the ambient runtime. Unset
`TS_EXPERIMENT_PIPELINE_THREAD`; hold
`TS_EXPERIMENT_TARGETED_REPLAY=1`, connected sockets, batched sends, topology,
buffers and placement fixed. `0` below means unset; `1` means exactly `1`.

| Mode | ACK_BATCH | RX_BATCH | NETSTACK_BUDGET | Main comparison |
|---|---:|---:|---:|---|
| base | 0 | 0 | 0 | Brief-15 item 3 baseline. |
| budget | 0 | 0 | 1 | Isolate the old driver/pump cost. |
| ack | 1 | 0 | 0 | ACK publication without the old budget. |
| ack-budget | 1 | 0 | 1 | Brief-15 ACK behavior. |
| rx | 0 | 1 | 0 | RX with receive-only protection; key candidate. |
| rx-budget | 0 | 1 | 1 | Brief-15 RX behavior. |
| both | 1 | 1 | 0 | Combined batching without full budgeting. |
| both-budget | 1 | 1 | 1 | Brief-15 combined behavior. |

Use median-of-three CPU-s/GB and throughput for all four workloads: 1-flow
down/up and 2×64 down/up. Retain each flow's start, duration, completion and byte
count, per-peer distributions, EOF tails, retransmissions, drops and memory.
Prioritize base/budget and RX/RX-budget on single-flow down, then the two RX
rows on 2×64 down/up. Add metrics-enabled attribution runs for those comparisons.
Compare new base against `aad9b22` base to detect incidental default-path cost.
Pipeline mode still implies full budgeting; toggling the new flag there is not
an independent budget experiment.

## Validation

All Cargo commands used `+1.95.0` and `--locked`. The brief-15 package suite
passed **1,033 tests across 27 test/doctest groups in each of nine runs**, with
zero failures or ignored tests: a default environment, then every ACK/RX/full
budget combination with targeted replay, connected sockets, batched sends and
pipeline metrics held on. This includes all 597 runtime tests and 16 forwarding
integration tests in each run.

```sh
cargo +1.95.0 test --locked \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp \
  -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime \
  -p geiserx_ts_magicsock \
  -p geiserx_ts_tunnel \
  --features geiserx_ts_runtime/ring-aead
```

Five added tests cover multiple TCP segments in a receive-only output turn,
receive-limit enforcement and short-drain yield behavior, dirty receive
completion, Ready/Pending output flushing and cancellation, and command/timer
service during a continuously readable receive-only device flood. Existing
native receive, ownership, STUN/disco/relay, listener-orphan, ABA, half-close and
fail-closed forwarding tests remain in the suite. Six pipeline lifecycle and
placement tests also passed with `PIPELINE_THREAD=1`, both batching flags on,
and the new budget flag off and on (six tests in each run).

Library Clippy for the affected packet path passed with `-D warnings`; the
core `--lib --no-default-features` check and `exit_node` example check passed.
Touched-file nightly rustfmt and `git diff --check` passed. Linux netstack
libraries with Tokio and magicsock library/test code cross-checks passed for
`x86_64-unknown-linux-gnu`. An initially broader Linux `--tests` check including
netstack dev-dependencies could not build `ring` because
`x86_64-linux-gnu-gcc` is absent; the passing cross-checks avoid that unrelated
native dev-dependency. No Linux runtime tests or performance benchmarks were
run on this Apple host.

Logs: `/tmp/codex-16-all-default.log`,
`/tmp/codex-16-all-a{0,1}-r{0,1}-b{0,1}.log` (ACK/RX/budget),
`/tmp/codex-16-pipeline-b{0,1}.log`, `/tmp/codex-16-clippy.log`,
`/tmp/codex-16-no-std-check.log`, `/tmp/codex-16-example-check.log`,
`/tmp/codex-16-linux-libraries.log`,
`/tmp/codex-16-linux-receive-tests.log`; the broader cross-check failure is in
`/tmp/codex-16-linux-check.log`.

`bd prime` could not run (`bd: command not found`); no tracker updates or sync
occurred. Linux checks use the previously verified 1.95.0 standard-library
sysroot under `/tmp`; Linux runtime execution requires a Linux host.
