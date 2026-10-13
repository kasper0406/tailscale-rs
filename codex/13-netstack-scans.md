# Brief 13: build your rank-1 netstack fixes (B item 1, then item 2 only if clean)

Your analysis is in `docs/codex-12-stage0-replan.md`. Build its **B item 1** now; Claude benchmarks.

Worktree: `/Volumes/git/ts-exit/scratch/wt-codex-scans`, branch `ts-exit/netstack-scans` (from
`ts-exit/bench-scaling` @ 818deb2). Work only there. Use `cargo +1.95.0`. Commit in small,
reviewable commits, message style like the existing history, each ending with:

    Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
    Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>

Do not push; Claude reviews and pushes.

Scope (item 1):
1. O(log n) (or O(1)) socket existence/type validation: replace the `socket_set.iter().any(...)` scans in
   `get_socket_mut!` and the TCP stream/close paths with the generation registry (extended with socket
   kind), maintained only by the `add_socket`/`remove_socket` wrappers. Keep F17 behaviors exactly
   (lossless FIN/Close, abandoned-receive skip, generation/ABA guard, the #297 recycled-slot type check).
2. `drain_tcp_closes`: dedupe `pending_tcp_closes`, use the registry, no per-close full scan.
3. `reap_orphaned_closed_tcp`: stop rebuilding listener ownership sets and scanning all sockets on every
   progressing poll. Maintain ownership incrementally and/or bound it (e.g. a sweep at most every N ms or
   when a socket transitions to Closed), with a reclamation deadline so idle/closed sockets are still
   freed promptly (existing tsr-9ue tests must keep passing).
4. Counters behind `TS_DEBUG_PIPELINE_METRICS=1` (or an equivalent cheap static): replay attempts,
   replay completions, validation lookups, orphan sweeps, so Claude can attribute changes on the harness.

Tests: unit tests for each change (including a many-socket case asserting no per-command scan, e.g. via a
visit counter in test builds), plus all existing tests:
`cargo +1.95.0 test -p geiserx_ts_netstack_smoltcp_core -p geiserx_ts_netstack_smoltcp_socket
-p geiserx_ts_netstack_smoltcp -p geiserx_ts_forwarder -p geiserx_ts_runtime --features geiserx_ts_runtime/ring-aead`
(run `ts_forwarder --test forwarding` several times; it has timing-sensitive tests). Format only files you
touch with `rustfmt +nightly --edition 2024`.

Item 2 (targeted waiter/listener replay via smoltcp recv/send wakers): only start it if item 1 is done,
green and committed; put it in separate commits so it can be benchmarked/reverted independently, and stop
with a written status if it gets risky.

When done, write `docs/codex-13-netstack-scans.md`: commits, what changed, test results, risks, and what
Claude should measure (expected effect per the 128-flow harness).
