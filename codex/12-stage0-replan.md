# Brief 12: re-rank the scaling plan against the stage-0 measurements (analysis only, no code)

You wrote `docs/codex-10-sharding-design.md` (staged plan: 0 foundations, 1 exit-flow sharding,
2 distributed WireGuard transport, 3 underlay ownership, 4 app services). Stage 0 is done. We now
have real measurements from an independent load generator through the live node. Read:

- `BENCHMARKS.md` §4 (results table, method, latency, conclusions) and raw JSON in
  `bench/results/stage0/{tokio4-fixed,pipeline1}/`
- `docs/DECISIONS.md` F16–F18, D18
- profile `scratch/prof/stage0-pipeline-down1.txt` (pipeline-thread mode, one download flow;
  `python3 scratch/categorize.py <file>` buckets it)
- code: fork checkout `repos/geiserx-tailscale-rs`, branch `ts-exit/bench-scaling` (HEAD 9b9ce9f).
  Relevant: `ts_netstack_smoltcp_core/src/lib.rs` (`poll_device_io`, `poll_device_ingress_async`,
  `IoPoller`, `pump_blocked_commands`), vendored `vendor/smoltcp` (`poll_egress` walks every socket),
  `ts_runtime/src/forwarder_actor.rs`, `ts_runtime/src/netstack_actor.rs`, `ts_magicsock` receive path.

Key facts:
1. Default node (4 tokio workers): one download flow 3.9 Gbit/s at 3.0 node CPU-s/GB; 2×8 flows
   7.3 Gbit/s; but CPU/GB grows with flows: download 3.0 → 5.8, upload 6.6 → 11.2 (1 → 64 flows per peer),
   and aggregate throughput falls past 8 flows. Node saturates ~3.4 cores.
2. Upload (node decrypts, netstack ingress, host write) costs ~2× download per GB. No `recvmsg_x` batching.
3. Single pipeline thread: 93% busy at 1.6 Gbit/s on one download flow, 2.4× CPU/GB (and the
   generator 3.4×: smaller bursts). But lower CPU/GB on upload and many flows, and p99 latency under load
   1.2 ms vs 13.6 ms on the default.
4. Goal (user): a generally scalable design, not tuning for this Mac.

Questions, answer concretely with file/function references and expected effect sizes:

A. Where does the per-flow superlinear cost come from? Confirm or refute: smoltcp `poll_egress` is
   O(sockets) per poll and we poll per packet batch; `pump_blocked_commands` replays every blocked
   command on every state change; anything else (listener/accept scans, `drain_tcp_closes`,
   `reap_orphaned_closed_tcp`, per-poll `poll_delay` over all sockets)? Estimate cost at 128 sockets.
B. What is the cheapest general fix for A that also helps after sharding (e.g. readiness/dirty-set
   driven egress and command replay, per-socket wakeups instead of global replay)? Can it be done in
   our netstack layer without forking smoltcp further, or does it need smoltcp changes? Risks.
C. Upload path: rank `recvmsg_x` batching vs other receive-side work (decrypt batching, fewer copies,
   coalesced ingress) by expected CPU/GB win.
D. Re-rank stages 1–4 plus B and C by (expected general scalability win) / (risk × effort). Is
   stage 1 (exit-flow sharding) still the right next build, or should B come first because sharding
   only divides the O(n) term by N? What measurement on our harness would decide it?
E. Anything in the stage-0 method that makes these numbers misleading (co-located generator, loopback
   sink, E-cores, redirect path differences vs real egress)? What should the stage-1 A/B add?

Write the answer to `docs/codex-12-stage0-replan.md` (concise, ranked, with the decision criterion).
Do not modify code or branches.
