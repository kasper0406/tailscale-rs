# Brief 16: decouple the work budget from the batching flags; keep the RX win, lose the download regression

Same worktree/branch (`scratch/wt-codex-scans`, `ts-exit/netstack-scans`, HEAD 77e0f36 + whatever you add).
Same rules (trailers, `cargo +1.95.0`, touched-file fmt, no push).

## Claude's A/B of brief 15 (same binary aad9b22, targeted replay on in every mode, CPU-s/GB median of 3)

| mode | 1 flow down | 1 flow up | 2×64 down | 2×64 up |
|---|---|---|---|---|
| base (item 3 only, flags unset) | 2.80 | 6.29 | 4.53 | 9.83 |
| ACK_BATCH=1 | **4.66** | 6.23 | 5.23 | 9.73 |
| RX_BATCH=1 | **4.54** | 5.44 | 5.22 | **8.14** |
| both | 4.63 | 5.45 | 5.15 | 8.05 |

Throughput: 1-flow down 3910 → ~3650 Mbit/s with either flag; 2×64 up 2608 → 2907 with RX. 2×64 download
flow-duration spread widened a lot with RX (min/med/max 1.92/6.14/8.15 s vs 4.60/5.48/6.25), i.e. unfairness.
Raw: `bench/results/stage0/batch-{base,ack,rx,both}/`. Item 3 (base vs previous binary) is a clean win
(2×64 up 10.57 → 9.83). The loopback sink does not explain the up/down asymmetry (sink CPU: 0.5 s per 6 GB
sent vs 5.8 s per 6 GB received).

Hypothesis: the download regression comes from the 128-item work budget + cooperative yields that *either*
flag turns on in the async netstack and runtime pumps, not from the batching itself.

## Do

1. Split the budget/yield behavior out of `TS_EXPERIMENT_ACK_BATCH` / `TS_EXPERIMENT_RX_BATCH` into its own
   flag (e.g. `TS_EXPERIMENT_NETSTACK_BUDGET=1`), so each can be measured alone. Pipeline-thread mode keeps
   its budget as before.
2. Check whether RX batching itself needs any budget for correctness or fairness (unbounded drains?). If it
   does, find the minimal bound that doesn't penalize a single download flow (e.g. bound only the
   receive/admission side, not netstack egress), and explain.
3. Look at why the budget path costs single-flow download ~65% more CPU/GB (e.g. egress emitted in small
   fragments, extra wakeups/yields per packet, `sendmsg_x` batch sizes collapsing). Fix it if cheap, or
   tell Claude which counters to read (`TS_DEBUG_PIPELINE_METRICS=1` histograms) to confirm.
4. Investigate the 2×64 download unfairness with RX batching (per-peer/per-socket starvation?).

Tests as before in all flag combinations you touch. Write `docs/codex-16-decouple-budget.md` with the
flag matrix Claude should run next.
