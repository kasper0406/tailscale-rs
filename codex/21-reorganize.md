# Brief 21: reorganize our work into clean, layered branches on the real GeiserX main

Goal (Kasper): clean branches we can show to maintainers. This is history surgery, not new features. Work in a
NEW worktree: `git -C /Volumes/git/ts-exit/repos/geiserx-tailscale-rs worktree add -b ts-exit/new-fixes
/Volumes/git/ts-exit/scratch/wt-codex-reorg origin/main`. Same rules: `cargo +1.95.0 --locked`, nightly rustfmt
on touched files, Signed-off-by `"Kasper Nielsen" <kasper0406@gmail.com>` on every commit, keep original authorship
and co-author trailers when cherry-picking, **no push and no deleting or rewriting existing branches** (Claude
does publishing). Never add public IP addresses anywhere (use 203.0.113.0/24 in tests).

## Why the base changes

Our `ts-exit/*` branches were rewritten with git-filter-repo today (an IP scrub). That also rewrote GeiserX's
own history (signatures stripped), so our branches sit on an unsigned duplicate of GeiserX history: `24c18fc`
is our copy of the real `719aa88`, with an identical tree. Rebuild everything by cherry-picking OUR commits onto
the real `origin/main` (GeiserX, currently 2 commits ahead of 719aa88). Our commits are
`24c18fc..ts-exit/linux-fixes` (93) plus `ts-exit/consolidated..ts-exit/shard-stage1` (shard-batch, metrics,
and the synthetic_peer STOP-wait and teardown fixes).

## Target layers (each branch builds on the previous one)

1. `ts-exit/new-fixes` (on origin/main): self-contained bug and security fixes that make sense to GeiserX
   without our exit-node runner. Candidates: HostExitDialer internet-only exit confinement; WakingPipeReceiver
   non-blocking try_recv; revoke sessions on static key/PSK change; remove expired peers from tunnel/routes;
   answer peer queries on a self-only netmap; complete Hostinfo on every map request; keep DNS view config/self
   node across deltas; ordered security bus with supervised consumers; vendored smoltcp + fast-retransmit RTO
   backport (+ its warning silencing); listener half-open RST orphan; abandoned receive; FIN/Close on a full
   command channel (unbounded default); Go betterAddr path scoring; the cancelled-task test assertion.
2. `ts-exit/new-exit-node` (on fixes): exit-node features and the runner: `examples/exit_node` (with
   `--interactive`, login wait, status lines, worker-threads default 4, DNS serving; squash the fmt/default-2→4
   churn), all-port ingress gate + generation-safe lifecycle, reject_ipv6 + runtime gating + rate limit, `::/0`
   advertisement, UDP flow keepalive (squash the first-millisecond reaper fix into it), exit-node DNS (peerAPI
   DoH, host resolver).
3. `ts-exit/new-perf` (on exit-node): proven performance work only: exitnode cargo profile, trace-level spans,
   batched egress handoff, 7 MiB UDP buffers, opt-in ring AEAD (+ bench), connected UDP sockets + sendmsg_x
   (Apple), registry socket validation + bounded orphan sweeps, targeted replay, RX batching (with only the
   receive-side bounds, i.e. the decoupled form), receive copy/alloc/tracing cleanup, splice coalescing, platform
   defaults (replay on everywhere; RX batch + coalesce on for Apple only), the Linux handoff/burst bounds,
   and the cheap metrics counters if they're needed by the above.
   Exclude: pipeline thread, full work budget, ACK batching, sharding, shard-batch.
4. `ts-exit/new-bench` (on perf): benchmark tooling: `ts_forwarder` forward_throughput benches (port modes,
   loss emulation, bounded lossy checks, finite transfer), the tunnel/echo benchmark sink in the runner if it's
   not already in layer 2, `TS_BENCH_EXIT_REDIRECT` dialer, `examples/synthetic_peer` with all its fixes (LAN
   endpoint advertise, enroll login link, hold sessions, hostname, progress on timeout, STOP wait = transfer
   timeout, best-effort teardown), the bounded A/B workloads, and the AEAD bench if it isn't in layer 3.
5. `ts-exit/new-experiments` (on bench): everything else: pipeline thread and its counters/bounds/coalescing,
   DERP-resolve-on-caller and supervision changes if they only matter for it, full work budget, ACK batching,
   sharding stage 1 + shard-batch + probes, and the in-repo `docs/codex-*.md` reports (these internal reports
   must NOT be in layers 1–4).

`ts-exit/new-main` = `ts-exit/new-perf` (the product the Mac runs). Use your judgment where a commit is
entangled (e.g. RX batching vs the budget decoupling commit, metrics counters used by experiments): split,
squash or re-cut commits so each layer is coherent and each commit has a clear message. Prefer fewer, cleaner
commits per feature over replaying churn (fmt fixups, follow-up fixes to our own bugs).

## Acceptance

- Every layer: `cargo +1.95.0 build --locked --workspace`, `cargo +1.95.0 clippy -p <touched crates> -- -D
  warnings` (or no new warnings vs origin/main), touched-file fmt check, and the suite from briefs 15–20
  (`ts_netstack_smoltcp*`, `ts_forwarder`, `ts_runtime`, `ts_magicsock`, `ts_tunnel`, `ts_dataplane` with
  `geiserx_ts_runtime/ring-aead`), plus the workspace tests that origin/main already passes (report any that
  fail on origin/main itself).
- Equivalence: with all env flags at their defaults, `new-main` must behave like current `ts-exit/linux-fixes`
  minus experiments. Produce `git diff ts-exit/linux-fixes ts-exit/new-main --stat` and explain every
  remaining difference (expected: removed experiment/bench/doc code only).
- `new-experiments` must contain everything from `linux-fixes` + `shard-stage1` (diff should be empty or
  explained).
- Write `docs/REORG-21.md` in the workspace repo path `/Volumes/git/ts-exit/docs/` (not in the code repo): the
  layer table (branch, head, commit count, one line per feature), the old→new mapping for each old commit
  (including which were squashed/dropped and why), test results per layer, and anything you couldn't resolve.
