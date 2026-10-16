# Brief 33: an instrumented exit-node build for a real-world pacing A/B (laptop over the internet)

Kasper wants real-world evidence, beyond our emulated bench (`docs/PACING-AB-32.md`), that pacing reduces
burst losses: his laptop on a real Wi-Fi/internet path will download through the live exit node, alternating
pacing off/on. We need one binary where pacing can be switched at runtime, plus TCP counters.

## Do

On a new branch `ts-exit/pacing-live` from `ts-exit/pacing-ab` (worktree under `/Volumes/git/ts-exit/scratch/`):

1. **Runtime toggle.** Replace the per-socket `std::env::var("BENCH_PACING")` read in
   `ts_netstack_smoltcp_core/src/socket_impl/tcp/mod.rs` with a process-global `AtomicBool` (default off), read
   when a TCP socket is created; existing sockets keep their setting. Keep the `BENCH_PACING` env as the initial
   value so the bench still works. Keep Reno as the default controller (`BENCH_CC` unchanged).
2. **Counters** in the vendored smoltcp (process-global atomics, relaxed ordering, no API break; this is a private
   instrumented build): data segments emitted (payload > 0), retransmitted data segments (any segment whose
   payload starts below the highest sequence already sent: fast retransmit and RTO replay), fast-retransmit
   events, RTO events, and paced deferrals (when the pacer holds back data the window would allow). Expose a
   snapshot function.
3. **exit_node example:** if `TS_PACING_CONTROL_FILE` is set, poll it once per second: content `on` or `off`
   sets the toggle; a missing file means off. If `TS_TCP_STATS_FILE` is set, rewrite it atomically (write a temp
   file, then rename) once per second with one JSON object:
   `{"ts_ms":…,"pacing":true|false,"data_segs":…,"retx_segs":…,"fast_retx":…,"rto":…,"paced_deferrals":…}`.
   Log the toggle changes at info.
4. Tests: a unit test that the toggle affects only sockets created after it changes; a smoltcp test that the
   counters count a fast retransmit and an RTO. Clippy `-D warnings`, fmt, the netstack/forwarder/smoltcp tests.
5. Build the exitnode binary (`--profile exitnode --features ring-aead --example exit_node`) and copy it to
   `/Volumes/git/ts-exit/bin/exit_node-pacing-live`. Confirm `--help` still lists `--serve-tcp` and
   `--tcp-buffer-size`.

## Constraints

Don't touch the live node, its daemon or `ts-exit/main`; don't push. Commits: `Signed-off-by: Kasper Nielsen
<kasper0406@gmail.com>`, `Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>`. `nice -n 10`, at most 4 jobs.

## Deliverable

A short `docs/PACING-LIVE-33.md`: branch and commits, the binary's SHA-256, the env vars and file formats, and
what each counter means exactly.
