# Brief 17: sharding stage 1 — exit-flow ownership across N forwarder stacks (opt-in)

Design: your `docs/codex-10-sharding-design.md` (conditions 1–7, stage 1 row) and re-rank
`docs/codex-12-stage0-replan.md`. Measurements so far: `BENCHMARKS.md` §4–§5, `docs/DECISIONS.md` F16–F24.

Worktree/branch: create a new branch `ts-exit/shard-stage1` from `ts-exit/netstack-scans` (HEAD 480a8c7) in a new
worktree `/Volumes/git/ts-exit/scratch/wt-codex-shard` (`git -C /Volumes/git/ts-exit/repos/geiserx-tailscale-rs
worktree add -b ts-exit/shard-stage1 ../../scratch/wt-codex-shard ts-exit/netstack-scans`). Same rules as before
(trailers, `cargo +1.95.0`, touched-file fmt, small reviewable commits, no push).

## Scope (stage 1 only)

- `TS_EXPERIMENT_FORWARDER_SHARDS=N` (read once at startup; unset/1 = today's single forwarder, byte-for-byte).
  With N>1: N forwarder netstacks, each driven on its own dedicated thread (current-thread Tokio runtime,
  named `ts-fwd-shard-<i>`), and the flow's gated listener, accepted-socket splice / host TCP stream, and UDP
  relay + reply pump run on that same shard runtime (colocation is the point; today splices spawn on the
  ambient runtime, `ts_forwarder/src/tcp.rs` / `forwarder_actor::spawn_forwarder`).
- Dispatch: one stable owner per flow for both directions, chosen from a keyed hash (per-process random key) of
  the canonical tuple namespaced by the authenticated peer (condition 1). SYN retransmits and all later
  packets of the flow land on the same shard; reply/egress from every shard goes back through the existing
  single WG/dataplane owner (stage 1 keeps one WG endpoint and one app stack). Fragmented packets: steer all
  fragments of a datagram consistently (condition 3, simplest safe rule; document it).
- Per-shard port gate / listener generations (condition 2). Node-wide admission limits must NOT multiply by N
  (share or partition the global flow semaphores; memory accounting per shard). IPv6 rejection and the exit
  destination guard stay before any shard, identical for all shards.
- Supervision (condition 7): a failed/exited shard stops forwarding globally (same fail-closed behavior as a
  failed forwarder today); joins on shutdown; security updates reach every shard with the existing ordered,
  guaranteed delivery (no fire-and-forget fan-out).
- Keep all the opt-in flags from §5 working and independent (targeted replay, RX batching, metrics). Pipeline
  thread mode and N>1 may be declared mutually exclusive (refuse to start, clear error) if combining is
  unsafe.
- Counters behind `TS_DEBUG_PIPELINE_METRICS=1`: per-shard packets in/out, flows, queue occupancy/drops.

## Tests

Shard selection stability (reversed tuples, SYN retransmit, peer namespacing, keyed hash), fragments, cold
port first SYN/datagram per shard, half-close/EOF across shards, global admission cap not multiplied,
shutdown/supervision (a panicking shard stops forwarding), security update reaching all shards, N=1
equivalence. Full suite from brief 15 with N unset and N=4 (+ the §5 flags on), `ts_forwarder --test
forwarding` several times each.

Write `docs/codex-17-shard-stage1.md`: commits, design choices vs codex-10, what is still serial (WG endpoint,
RX), expected scaling ceiling, the exact A/B matrix for Claude (N=1 vs previous, N=1/2/4/8 at 1×1, 2×8, 2×64
flows both directions, with §5 flags fixed on).
