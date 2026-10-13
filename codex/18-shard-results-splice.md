# Brief 18: stage-1 results, and coalesced host writes in the splice

Branch `ts-exit/shard-stage1` (worktree `scratch/wt-codex-shard`, HEAD 0753c5c; Claude pushed it). Same rules.

## Claude's A/B (metrics off; targeted replay + RX + ACK batching on everywhere; 3 runs each, median)

| load/dir | prev (85dd5f5) | N=1 | N=2 | N=4 | N=8 |
|---|---|---|---|---|---|
| 1×1 down Mbit/s (CPU-s/GB) | 4426 (2.71) | 4300 (2.80) | 2871 (2.20) | 3307 (2.15) | 3300 (2.16) |
| 1×1 up | 2561 (5.37) | 2500 (5.49) | 2552 (5.68) | 2546 (5.65) | 2563 (5.69) |
| 2×8 down | 7744 (2.84) | 7330 (3.04) | 6259 (2.52) | 6258 (2.72) | 6161 (2.81) |
| 2×8 up | 3953 (5.97) | 3829 (6.15) | 3216 (8.82) | 3027 (10.30) | 2113 (15.13) |
| 2×64 down | 5029 (4.65) | 5057 (4.54) | 5043 (4.56) | 5676 (3.11) | 5518 (3.18) |
| 2×64 up | 3008 (7.96) | 3018 (7.88) | 2662 (9.82) | 2242 (13.44) | 2034 (15.36) |

Raw: `bench/results/stage0/shard-{prev,n1,n2,n4,n8}/`. No stalls/errors anywhere. N=1 ≈ prev (good).
Only 2×64 download benefits (N=4: −33% CPU/GB, +13%). Upload CPU/GB roughly doubles by N=8; single-flow and
2×8 download lose throughput.

Profile N=4, 2×64 upload (`scratch/prof/shard4-up-2x64.txt`): `__sendto` 4171 samples (vs 2644 in the
non-sharded profile at higher throughput), shard threads 61–66% busy, tokio workers 42–45% busy. Hypothesis:
host TCP writes got smaller: each shard thread writes a segment's worth to the host socket as soon as it
arrives, whereas the busier shared pool used to let more accumulate per read. Single-flow download loss is
likely the added cross-thread hop's latency on a window-limited (256 KiB, no autotuning) flow.

## Do

1. **Coalesced splice writes (general, benefits N=1 too):** in the forwarder's overlay→host direction (and
   symmetric if useful), gather all immediately readable netstack data (up to e.g. 64–256 KiB) before one host
   `write`, without waiting for more data (never add latency: when the next read would block, write what you
   have). Keep exact half-close/EOF semantics of `copy_bidirectional` (FIN propagation both ways, F17 lossless
   shutdown), error behavior and backpressure. Consider larger netstack Recv max_len and host read sizes for the
   other direction. Flag it (`TS_EXPERIMENT_SPLICE_COALESCE=1`) so Claude can A/B it, then measure write sizes
   with a cheap counter (bytes per host write histogram behind the metrics flag).
2. Confirm or refute the upload hypothesis with that counter; if other causes appear (cross-thread wake storms,
   the bounded WG handoff, per-packet dispatch hops), report them with evidence and fix the cheap ones.
3. Assess the single-flow download loss at N>1: verify the latency explanation (e.g. echo RTT through a shard vs
   N=1) and propose the cheapest mitigation (batch handoffs, avoid hop for the dominant flow?) without
   committing to stage 2/3 yet.

Tests as before (all modes incl. N=4 and the new flag). Write `docs/codex-18-splice-coalesce.md` with the next
A/B matrix (keep it small: prev-N=1 vs coalesce at N=1 and N=4; 1×1, 2×8, 2×64, both directions).
