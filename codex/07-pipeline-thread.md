Your ring AEAD patches are reviewed and merged (opt-in feature). Since then (all pushed to
github.com/kasper0406/tailscale-rs): sendmsg_x batched sends on connected sockets (opt-in env
TS_DEBUG_MAGICSOCK_BATCHED_IO, ts_magicsock/src/batch.rs), netstack egress batching, 7 MiB UDP buffers,
Go betterAddr path scoring, trace-level hot-path spans, runner --worker-threads (default 4).
Read BENCHMARKS.md section 1c and docs/DECISIONS.md D14/D15 first.

Key measurement (live, tunnel-only, Go client -> Rust node over LAN, connected+DF+batching+ring):
  tokio workers 10: ~9.1 CPU-s/GB single flow; 15.2 with 8 flows; echo p99 12 ms under load
  workers 4: ~9 single; 10.6 with 8 flows; p99 12 ms
  workers 2: 6.4 single, but 8 flows -> 3-4 s stalls (serial pipeline starved by per-flow tasks;
             netstack ingress queue (4096) drops -> TCP RTOs)
  workers 1: 3.6 single (61% less than 10), but stalls even with one flow.
Profiles (macOS `sample`, scratch/prof/sample*.txt; categorize with scratch/categorize.py) show tokio
park/unpark (psynch_cvsignal/cvwait), kevent, mach_absolute_time, work stealing.

Goal: give the serial packet pipeline its own dedicated thread so it is neither starved by bulk
per-flow tasks nor bounced across cores: magicsock receive pumps (incl. connected-socket pumps) ->
run_direct -> dataplane -> application netstack + forwarder netstack poll loops and their ingress/egress
pumps (and the DERP/multiderp data hand-off if it is on the hot path). Everything else (control, per-flow
TCP splice / UDP relay tasks in ts_forwarder, peerapi/DNS, listeners, status) stays on the caller's
multi-thread runtime. Branch: ts-exit/pipeline-thread based on ts-exit/perf-connected HEAD.

Steps:
 1. Map precisely (file:line) which tasks/actors carry each packet and how they are spawned (kameo actors
    via tokio::spawn in the ambient runtime; netstack run_tokio; pumps in forwarder_actor/netstack_actor;
    DirectTransport pump; run_direct). Identify the minimal cut: e.g. a dedicated OS thread running a
    current_thread tokio runtime (or a LocalSet) on which those tasks are spawned, with the existing
    channels as the boundary to the rest. Note anything that blocks (sync locks held long, blocking calls)
    and anything that must stay Send/'static.
 2. Implement behind an explicit opt-in: a Config/Env field (e.g. `dataplane_thread: bool`) or env knob
    `TS_EXPERIMENT_PIPELINE_THREAD=1`; default behaviour must be byte-for-byte unchanged. Shutdown must
    still be clean (the dedicated runtime is dropped/joined on Runtime shutdown; no leaked thread).
    Security supervision from blocker 5 (SecurityGuard aborting the dataplane pump / forwarder) must still
    work for tasks on the dedicated thread — verify with its existing tests.
 3. Tests: all ts_runtime/ts_dataplane/ts_forwarder/ts_magicsock tests pass with the knob off AND on (find
    a way to run the runtime test suite with it on). Add a test that the pipeline tasks actually run on the
    dedicated thread (thread name) and that a CPU-hogging per-flow task on the main runtime cannot delay
    pipeline progress (bounded).
 4. Build the example: `cargo build --profile exitnode --example exit_node --features ring-aead` and
    hand off; I (Claude) will run the live A/B (runner/tunnel-bench.sh, 8-flow concurrency, rttprobe).
Deliver format-patches in scratch/codex-pipeline-thread/ + README (design, what moved, risks).
DCO `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`. Own worktree; CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target.
Do not touch the live node, the runner checkout or the Go client.
