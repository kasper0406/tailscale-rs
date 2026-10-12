# Brief 13: netstack validation and bounded reclamation

2026-10-12. **B item 1 is built, green and committed.** Work stayed in
`/Volumes/git/ts-exit/scratch/wt-codex-scans`, branch `ts-exit/netstack-scans`,
based on `ts-exit/bench-scaling` at `9b9ce9f`. No push. Claude reviews and benchmarks.
The supplied replan was read from the parent checkout's
`docs/codex-12-stage0-replan.md`; that file is absent from this worktree's base.

| Commit | Change |
|---|---|
| `ae356e6` | `ts_netstack_smoltcp_core: validate socket kinds through the generation registry` |
| `9d987f0` | `ts_netstack_smoltcp_core: bound orphan sweeps with an idle reclamation deadline` |
| `ec93a46` | `examples/exit_node: report netstack replay and sweep counters` |

All implementation commits carry the requested Signed-off-by and GPT-6.1
Co-Authored-By trailers. This document is committed separately.

**What changed.** The existing generation map now stores generation and socket
kind. Its production contents are maintained only by `add_socket` and
`remove_socket`. TCP, UDP, raw and ICMP typed validation uses one BTreeMap lookup
before smoltcp's indexed getter: O(log n), with no socket-set walk. This covers
`get_socket_mut!`, connect replay, TCP stream commands and UDP/raw close guards.
The #297 recycled-slot type checks remain in both directions. Generation/ABA
replay checks, abandoned-receive skips and lossless FIN/Close delivery remain.

Pending TCP closes use a BTreeSet, deduplicating at every insertion. Draining
validates through the registry, then checks the indexed socket state. Removing
a socket clears pending-close and connecting-set membership before slot reuse.
Close work is O(c log n) for c distinct pending closes, rather than O(c × n).

Orphan reclamation runs at most once per 100 ms per stack with live TCP sockets.
Listener ownership is snapshotted only during that bounded pass. The pass walks
live registry entries instead of smoltcp's storage high-water mark; pending-close
membership is logarithmic. It still excludes listener-owned sockets, half-closed
states and Closed sockets with unread RX. `poll_at`/`poll_delay` include the
reclamation deadline without hiding earlier TCP timers. Both synchronous and
async I/O drivers service reclamation even without packet progress or transmit
capacity. An eligible consumer socket is reclaimed on the next sweep, within
100 ms while the driver is serviced.

Outstanding connects are tracked separately and checked during each drain.
Failed, rx-empty Closed connects are reclaimed promptly; established connects
leave this set. This preserves the existing IPv6 first-SYN rejection test's
`BadSocketHandle` result. The first full test run exposed `ConnectionReset`
instead when all reclamation was delayed; the final implementation fixes that
behavior without restoring a full scan per progressing poll.

**Counters.** Set `TS_DEBUG_PIPELINE_METRICS=1` before process startup. The flag
is cached once, then copied into each stack; disabled recording tests a bool.
With the core's `std` feature off, counters stay disabled. Runtime builds enable
that feature. Counters are cumulative, process-wide across all netstacks, and
use relaxed pointer-width atomics (cast to u64 in snapshots).

The existing runtime pipeline snapshot now contains `netstack`, also printed
by the exit-node example's cumulative status log. Use `--status-interval 1`
for shorter sampling windows; the example defaults to 30 seconds. The core API
is `ts_netstack_smoltcp_core::pipeline_metrics::snapshot()`.

| Field | Counts |
|---|---|
| `replay_attempts` | Blocked commands actually executed again; excludes abandoned receives and stale-generation rejects. |
| `replay_completions` | Executed replays returning a terminal response, including errors. |
| `validation_lookups` | Kind/membership and generation lookups; includes requeue generation captures. |
| `orphan_sweeps` | Bounded full orphan passes. |
| `orphan_socket_visits` | Live registry entries visited by those passes, including non-TCP entries skipped by kind. |

**Validation.** All commands used Rust 1.95.0. The requested final full suite
passed **734 tests across 20 test binaries/doctest groups**, zero failures or
ignored tests, including all **597 runtime tests**:

```sh
cargo +1.95.0 test \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp \
  -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime \
  --features geiserx_ts_runtime/ring-aead
```

- `cargo +1.95.0 test -p geiserx_ts_forwarder --test forwarding`: five additional
  consecutive passes, 16/16 each (80 additional test executions).
- `TS_DEBUG_PIPELINE_METRICS=1 cargo +1.95.0 test -p geiserx_ts_netstack_smoltcp_core --features std`:
  39/39 including integration tests and doctests. Gate/snapshot tests also ran
  with metrics disabled in the full suite.
- `cargo +1.95.0 check -p geiserx_ts_netstack_smoltcp_core --lib --no-default-features`: passed.
- `cargo +1.95.0 check --example exit_node --features ring-aead`: passed.
- `rustfmt +nightly --edition 2024 --check --config skip_children=true` on only
  touched Rust files: passed. `git diff --check`: passed.

The core now has 33 unit tests, up from 23. Added cases cover all registered
kinds and generation changes on slot reuse, stale pending-close kind rejection,
512 sockets receiving 2,560 direct stream commands and 512 close validations
with zero sweep visits, and a storage-churn case with 511 vacant slots but just
one registry visit. Further tests cover sweep frequency, the reclamation deadline,
idle synchronous/async drivers, earlier TCP timers, prompt failed-connect cleanup,
replay attempt/completion accounting and the environment gate. Existing tsr-9ue,
ABA, abandoned-receive, duplicate-close and forwarding half-close tests pass.
Existing orphan-reap unit tests now advance to the sweep deadline rather than
expecting an unconditional sweep at a frozen timestamp.

Local validation logs are `/tmp/codex-13-all-tests.log`,
`/tmp/codex-13-forwarding-{1,2,3,4,5}.log`,
`/tmp/codex-13-metrics-tests.log`, `/tmp/codex-13-no-std-check.log` and
`/tmp/codex-13-example-check.log`. These are local artifacts, not tracked files.
`bd prime` could not run (`bd: command not found`); no tracker issue was created,
updated or closed, and no tracker sync was attempted.

**Risks and item 2 status.** Reclamation of ordinary rx-empty Closed consumer
sockets may now lag closure by up to 100 ms plus driver scheduling delay. TCP
stacks, including listener-only stacks, acquire a maintenance wake at roughly
10 Hz when otherwise idle. Ownership snapshots still allocate and scan listener
queues at that cadence. BTreeSet close bookkeeping trades allocation overhead
for deduplication. There is no change to packet matching, smoltcp egress scans,
timer-selection scans, or global waiter/listener replay; their residual costs
remain measurable. Enabled validation counters add atomic work, and snapshots
are cumulative rather than an atomic cross-field sample.

Item 2 has no implementation or feature change. After item 1 was green and
committed, the readiness approach was assessed and stopped at the risk boundary.
smoltcp permits only one waker per direction and consumes each registration on
wake. A safe fan-out must preserve FIFO waiters and generations, rearm correctly,
clean canceled receives on sockets with no future wake, and promote listeners
before the next SYN. The current response-channel cancellation check supplies
no dedicated ready-queue notification. A partial conversion could retain canceled
requests indefinitely or stall connection/accept progress. Keep item 1 as the
isolated benchmark candidate; targeted replay needs separate implementation and
lifecycle tests.

**What Claude should measure.** Compare `9b9ce9f`, `ae356e6` and `ec93a46` on the
same default topology and buffers. Keep metrics-disabled paired performance
runs separate from metrics-enabled attribution runs. Use warm, equal-duration
windows at 1/16/128 active flows, both upload and download, then add 128/512 idle
established sockets without changing the active load. Include churn and finite
transfers that wait for sink EOF. Report node CPU-s/GB, throughput, latency,
drops/retransmissions, completion tails and memory; record socket count and
storage high-water mark where available.

At 128 uniformly positioned sockets, the replan estimates about 8,256 old
validation slot visits for one blocked command per socket per pump. Those type
checks become 128 BTreeMap lookups; generation checks and requeue captures are
additional logarithmic lookups, so `validation_lookups` will exceed 128. Pending
closes similarly lose their nested scan. In a stable established-flow run,
`orphan_sweeps` should be at most roughly 10/s per TCP-bearing stack and
`orphan_socket_visits` should scale with live registry size × sweep count,
rather than packet-progress polls. Replay attempts/completions should remain
similar because item 2 is unchanged.

Expected effect remains the replan's **10–25% high-flow node CPU reduction if
validation is a substantial share**, with negligible single-flow gain. This is
a hypothesis; no benchmark was run here and residual idle-socket slopes should
remain from smoltcp's other scans. Apply the proposed gate of at least 15% lower
128-flow CPU/GB, at most 5% single-flow throughput loss, preserved delivery/EOF,
and no matched-load latency or drop regression. Do not add item 2's hypothetical
saving to this range.
