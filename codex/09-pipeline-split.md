Thanks for the critique — it was right on the risk. Controlled A/B results (same binary ba189ab, knob
0/1/1/0, 4 caller workers, 256 KiB; bench/results/pipeline-controlled.txt, BENCHMARKS.md "Dedicated
packet-pipeline thread"): single download CPU/GB -47% (8.85 -> 4.7) at +12% throughput; single upload -33%
at -7% throughput; 8 flows CPU/GB about -35% but aggregate -28% (~1715 -> ~1235 Mbit/s) and echo stalls in 2/4
runs vs 1/4. `ps -M` during 8 flows: pipeline thread 86% of a core, the four caller workers ~5% each.
The user approved building the follow-up. Branch: ts-exit/pipeline-split, based on ts-exit/pipeline-thread
(ba189ab) rebased onto current ts-exit/perf-connected HEAD (includes runner --worker-threads default 4).

Build:
 1. Two dedicated threads instead of one, still opt-in (keep the env knob; e.g.
    TS_EXPERIMENT_PIPELINE_THREAD=2 for the split, =1 the current single thread, unset = off). Choose the cut
    from evidence (profile the single-thread mode under 8 flows: scratch/prof, `sample <pid>`), likely:
    thread A = underlay (magicsock recv pumps incl. connected sockets, run_direct, DERP data tasks, batched
    sends) + dataplane/WireGuard crypto; thread B = application + forwarder netstack poll loops and their
    ingress/egress pumps. Keep cross-thread hand-offs batched (the existing per-batch channels), never
    per-packet.
 2. Your safety items: bounded drains/time or packet budgets with yields in netstack process_cmds /
    IoPoller ingress so one burst cannot monopolise a pipeline thread (measure that it doesn't cost
    throughput); security supervision that cannot be starved by a packet flood on the pipeline threads
    (e.g. monitors on their own tiny runtime/thread, or back on the caller with priority reasoning — your
    call, justify it); DERP UseDns resolution on the caller runtime with sockets still registered on the
    pipeline runtime, so shutdown is bounded under a resolver stall (test it).
 3. Benchmark-harness fixes you asked for: per-flow bytes + completion time, probe window separated from
    the throughput window, an 8-upload variant, and recording of binary hash/flags. Put them in
    runner/ab-matrix2.sh (+ python helpers under runner/) — scripts only, I run them. Also expose cheap
    counters I can read after a run without hot-path tracing: netstack ingress drops (per stack), egress
    batch-size histogram, sendmsg_x batch histogram, pipeline-thread busy time — e.g. a periodic info log
    line from the runner every --status-interval, gated by an env var.
 4. Tests: all four crates (runtime, dataplane, forwarder, magicsock) with knob unset/1/2 (ring-aead on);
    new tests for budget fairness (a flooding stack cannot delay another task beyond a bound), supervision
    under flood, and bounded shutdown under a stalled resolver.
 5. Build `cargo build --profile exitnode --example exit_node --features ring-aead`, copy the binary to
    scratch/codex-pipeline-split/exit_node with its sha256.
Bar for proposing it as default (I'll measure live with your harness): vs knob unset with 4 workers —
single-flow CPU/GB at least 30% lower, 8-flow aggregate down AND up within 5% or better, no echo stalls
(>200 ms) in 8/8 loaded runs, p99 echo under load no worse. Say honestly if you think the bar is wrong.
Deliver format-patches + README in scratch/codex-pipeline-split/; DCO `Signed-off-by: "Kasper Nielsen"
<kasper0406@gmail.com>`; own worktree; CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target. Do not
touch the live node, the runner checkout, or the Go client.
