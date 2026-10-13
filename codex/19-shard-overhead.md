# Brief 19: find and fix the cross-thread overhead that makes sharded upload expensive

Worktree `scratch/wt-codex-shard`, branch `ts-exit/shard-stage1` (HEAD 179e151, pushed). Same rules.
Claude is separately consolidating the N=1 wins on another branch; do not change flag defaults here.

## Evidence (Claude, harness §4, all with targeted replay + RX + ACK batching on)

| 2×64 upload | CPU-s/GB | Gbit/s |
|---|---|---|
| coalesce, N=1 | 7.87 | 3.01 |
| coalesce, N=4 | 13.73 | 2.22 |
| no coalesce, N=8 | 15.36 | 2.03 |

2×8 upload is the same story (N=1 5.78 → N=4 10.86). Download at N=4 is fine or better. Raw:
`bench/results/stage0/{shard-*,coal-*}/`.

Attribution run, N=4 + coalesce + metrics, 2×64 upload (log line in `scratch/co4-up-metrics.txt`, profile
`scratch/prof/co4-up-metrics.txt` taken with `sample` during the same load): shard dispatcher drops ≈ 0
(1 drop in ~4.2 M shard packets_in), peaks 438–4096, packets_out/in ≈ 0.45. So it is not loss or retransmission
from dispatcher drops. Earlier N=4 profile (`scratch/prof/shard4-up-2x64.txt`): shard threads 61–66% busy, the
4 tokio workers 42–45% busy, host-write `__sendto` 3938 samples (+233 UDP), `sendmsg_x` 951, `recvmsg_x` 1108,
`__psynch_mutexwait` 535.

## Do

1. Attribute the extra ~6 CPU-s/GB at N=4 upload: per-thread call graphs for the shard threads and workers,
   wakes/notifications per packet across the dispatcher and the bounded WG handoff, egress batch sizes per
   shard (do 4 shards each send small ACK batches → many more, smaller `sendmsg_x`/encrypt calls?), lock
   contention (`__psynch_mutexwait` callers), timer/poll churn per shard (each shard's 10 Hz maintenance,
   `poll_delay` scans), host-write sizes per shard. Use counters you add behind the metrics flag; Claude can
   run attribution loads for you if you give exact commands, but you can also drive the in-process probe you
   built in brief 18.
2. Fix the cheap causes (e.g. coalesce shard→WG handoff batches across shards before sealing/sending, avoid
   per-packet cross-thread wakes, batch dispatch to shards). Keep fail-closed/security properties from
   brief 17 unchanged.
3. Re-check single-flow download at N>1 (4.35 → 2.7–3.3 Gbit/s, while your probe says +5 µs RTT). Find the
   real cause.

Tests as in brief 17/18 for every mode you touch. Write `docs/codex-19-shard-overhead.md` with evidence,
fixes, and a small A/B matrix (N=1 vs N=4, 1×1/2×8/2×64, both directions, coalesce on).
