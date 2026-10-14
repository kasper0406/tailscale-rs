# Brief 30: a sane local workaround for smoltcp's missing loss recovery on GeiserX main

## Context

The live exit node (`kmini`) now runs **GeiserX/tailscale-rs main + one commit** (`ts-exit/main` 0e10bc3 in
`/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`; GeiserX main 18fc6bc uses crates.io **smoltcp 0.14.0**
with Reno pinned, `docs/review-28-design.md`). Our earlier fix for multi-second TCP stalls under burst loss
(`docs/TCP-RECOVERY-24.md`) was a patch series on a vendored smoltcp 0.13.1, which GeiserX didn't take. Your
brief-29 analysis (`docs/SMOLTCP-UPSTREAM-29.md`) found that most of our congestion-control fixes are upstream
already, that B1 (failed emit loses the fast retransmit) and B2 (zero-window reopen drops the RTO) are still bugs
on smoltcp main, that NewReno and sender SACK are missing features, and that `set_min_rto` exists on smoltcp main
(#1211) but not in 0.14.0. smoltcp's contribution policy forbids LLM-authored PRs, so upstream gets human-written
bug reports (Kasper is writing those). **This brief is only about our private build**, where that policy doesn't
apply.

Goal: the simplest sane way for our build of GeiserX main to stop stalling on lossy client links and to support a
larger window again (single flow was ~9 MB/s at 256 KiB vs 17–20 MB/s at 1 MiB with our old patches, F35), with a
small, auditable, easily dropped delta.

## Do

1. **Branch** `ts-exit/tcp-0.14` from `ts-exit/main` in a worktree under `/Volumes/git/ts-exit/scratch/`. Bring
   over the bottleneck bench (`ts_forwarder/benches/forward_throughput.rs` with `BW_MBPS`/`QUEUE_KB`, recovery-gap
   and latency-probe reporting; it lives on tag `archive/ts-exit/bench-pre-geiserx-switch` and branch
   `ts-exit/tcp-recovery`) as a bench-only commit.
2. **Baseline:** stock 0.14.0 as GeiserX configures it (Reno). Matrix: RTT 25 ms; BW 150 and 250 Mbit/s; queue
   64/128/256 KiB; window 256 KiB and 1 MiB; 1 and 8 flows; down and up. Plus no-loss rows (RTT 0 and 40 ms/4 MiB,
   1 and 8 flows). Report goodput, longest pause, and loaded-probe p95.
3. **No-patch mitigations**, measured on the same matrix: smoltcp's device `max_burst_size` (where does
   GeiserX's netstack set device capabilities?), Reno vs Cubic, keepalive interval as a B2 backstop, and the window
   size. Say how far these get.
4. **Patched 0.14**, as a minimal series on the 0.14.0 source, wired in with `[patch.crates-io]` (vendored under
   `vendor/smoltcp` or a path; recommend which, and pin it), each commit small and justified:
   B1 fix; B2 fix; backport of `set_min_rto` (#1211) so the netstack can use 200 ms; NewReno partial-ACK recovery;
   the bounded SACK scoreboard **including the review-26 validation fix and walk bounds** (`4e6b554`); pacing only
   if the matrix shows it still matters. Reuse our tests; keep smoltcp's existing tests passing. Enable the policy
   in the netstack (min RTO, SACK, pacing) in a separate small commit.
5. **Safety:** re-run the reviewer's randomized socket harness against the patched scoreboard (scratch copies
   `review26b-copy`, `review26b-old`, `review26b-base`; see `docs/review-26-tcp-recovery.md`) for at least as many
   rounds as before; no panics, no busy loops. Check no_std builds of the patched crate.
6. **Recommend:** stock+mitigations vs patched, the window size to run, and what the delta costs to carry until
   upstream catches up (rebase effort on a future 0.15).

## Constraints

- Don't touch the live node, its daemon or `ts-exit/main`; don't push or touch remotes.
- Commits: `Signed-off-by: Kasper Nielsen <kasper0406@gmail.com>`, `Co-Authored-By: GPT-6.1 (codex)
  <noreply@openai.com>`. No public IP addresses.
- Workspace clippy `-D warnings`, fmt, tests for touched crates. `nice -n 10`, at most 4 jobs; run benchmarks
  sequentially with nothing else heavy running.

## Deliverable

`/Volumes/git/ts-exit/docs/TCP-WORKAROUND-30.md`: the matrix (stock / mitigations / patched), the fuzz result,
the branch and commit list, and the recommendation. Concise.
