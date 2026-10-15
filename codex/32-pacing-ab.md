# Brief 32: a clean A/B of pacing alone on stock smoltcp 0.14.0 (numbers for an upstream issue)

Kasper wants to open a smoltcp issue proposing an opt-in pacer. His draft quoted numbers that mixed several
changes (window size, NewReno, SACK, Cubic fixes and pacing), so we need a clean A/B: **stock smoltcp 0.14.0 vs
stock 0.14.0 + only a pacer**, on the bottleneck bench. You built both the pacer and the bench (brief 30,
`docs/TCP-WORKAROUND-30.md`, branch `ts-exit/tcp-0.14`).

## Do

1. New branch `ts-exit/pacing-ab` in a worktree under `/Volumes/git/ts-exit/scratch/`, from `ts-exit/main`'s base
   `0e10bc3` (GeiserX main + serve), plus: the bench-only commit (`9af3f91`), the pristine 0.14.0 vendor import
   with `[patch.crates-io]` (`038876f`), and **only the pacer**, adapted so it works without NewReno, SACK, the
   B1/B2 fixes or the RTO backport. Keep the pacer's behaviour as in `21e8417` + `92c7b2e` (rate 1.2 × cwnd /
   SRTT, bounded burst, ACKs and retransmissions unpaced, bypass without a valid RTT or at baseline RTT ≤ 5 ms,
   debt capped at one SRTT). Hook it only where stock smoltcp's own loss signals exist (fast retransmit entry,
   RTO). Netstack: a bench/env switch selects pacing on/off and Reno/Cubic. Everything else stays stock (1 s
   minimum RTO, stock recovery).
2. Sanity: smoltcp lib tests pass with the pacer enabled and disabled; a quick check that pacing actually engages
   (e.g. count paced deferrals).
3. Matrix, **down** direction: RTT 25 ms; BW 150 and 250 Mbit/s; QUEUE_KB 64, 128, 256; TCP_BUF_KB 256 and 1024;
   flows 1 and 8; variants {stock, stock+pacer} × {Reno, Cubic}. **Three repetitions per cell**, interleaved
   (A, B, A, B...) to spread out machine noise. Also an **up** subset (Q64 and Q256, 1 MiB, 1 and 8 flows) and
   no-loss controls (RTT 0 / 256 KiB and RTT 40 ms / 4 MiB, 1 and 8 flows) to show pacing doesn't cost
   throughput where there's no bottleneck. 15 s per lossy run, 30 s per no-loss run. Run sequentially with nothing
   else heavy; nice -n 10; at most 4 build jobs.
4. Report per cell: median goodput (with min–max), longest pause, queue drops (the bench counts tail drops), and
   loaded-probe p95. Then a short **summary table for the issue**: e.g. median across the lossy cells, a few
   representative rows (single flow at Q64/W1M, 8 flows at Q64 and Q256), and the no-loss controls.

## Constraints

Don't touch the live node, `ts-exit/main` or remotes; no PRs. Commits: `Signed-off-by: Kasper Nielsen
<kasper0406@gmail.com>`, `Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>`. No public IPs. These numbers
are for an issue describing the problem; the pacer code itself won't be submitted (smoltcp requires
human-written code), so don't polish it beyond correctness.

## Deliverable

`/Volumes/git/ts-exit/docs/PACING-AB-32.md`: setup (exact versions, bench parameters, what "pause" and "drops"
mean), the summary table, the full matrix, and caveats (both ends of the bench are smoltcp; no WireGuard/TLS; one
machine). Concise.
