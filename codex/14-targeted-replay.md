# Brief 14: targeted waiter replay (your B item 2), opt-in, with a full-sweep safety net

Same worktree/branch as brief 13: `/Volumes/git/ts-exit/scratch/wt-codex-scans`, `ts-exit/netstack-scans`
(HEAD 497b5fd). Same commit trailers, no push, `cargo +1.95.0`, format only touched files.

## Evidence (Claude measured your item 1 on the harness)

- 2 peers × 64 flows, download: node CPU 5.75 → 5.37 CPU-s/GB (−7%), throughput +5%; upload 11.15 → 11.00
  (−1%); 1–8 flow cells unchanged. Below the 15% gate, so item 1 alone is not enough (still worth keeping).
- Profile after item 1 (`scratch/prof/scans-down-2x64.txt`, 2×64 download): `process_one_cmd` self
  1919 → 725 samples, new `BTreeMap::get` 365, `pump_waiters` 458; netstack bucket 25.6% → 20.6% of busy.
- Your counters under the same load (cumulative over one ~15 s run):
  `replay_attempts 261,215,383`, `replay_completions 2,177,415`, `validation_lookups 784,847,687`.
  **~120 replay attempts per completion: 99.2% of replays are wasted.** Global replay volume is now the
  dominant avoidable netstack cost.

## Design to build (adjust if you find a correctness problem, and say why)

- Enable vendored smoltcp's `async` feature for the core. Verified wake points in
  `vendor/smoltcp/src/socket/tcp.rs`: `set_state` wakes rx+tx (1391–1404), data arrival wakes rx (2176),
  ACK freeing tx space wakes tx (2031, 2684), plus 918–919. UDP has `register_recv_waker`/`register_send_waker`.
- Blocked commands live in per-handle FIFO queues keyed by `(handle, generation)`; handle-less commands
  and socket kinds without wakers (raw/icmp, listeners/accept) keep the existing always-replay behavior.
- When a command returns WouldBlock, register a cheap custom waker for the direction it waits on
  (Recv → rx; Send → tx; Connect → both). The waker only marks the handle dirty in a shared set
  (registrations are one-shot; re-register on every requeue). Pumps replay only dirty handles' queues, in FIFO
  order, preserving the existing generation/ABA guard, abandoned-receive skip and lossless FIN/Close.
- **Safety net:** a full replay of all queues at a bounded cadence (e.g. every 50 ms, and on timer wakeups),
  so a missed wake costs latency, never a stall. Count safety-net completions separately: in a correct build
  they should be ~0 under load; nonzero means a missed wake path to fix.
- Cancellation: when enqueuing for a handle, purge that handle's abandoned receives (bounded growth with a
  `timeout(recv)` retry loop on a socket that never wakes); the full sweep purges too.
- Removing a socket drops its queue (answering `missing_socket` exactly as today).
- Opt-in env flag `TS_EXPERIMENT_TARGETED_REPLAY=1` (read once at startup, like the metrics flag) so Claude
  can A/B the same binary; default behavior stays byte-for-byte the current global replay.
- Counters: dirty handles pumped, targeted replay attempts/completions, safety-net sweeps/attempts/completions.

## Tests

Unit tests: data arrival completes a blocked Recv without a full sweep; ACK space completes a blocked Send;
connect completion; remote FIN completes Recv with Finished; RST/close; generation reuse with a queued
command; abandoned receives are purged; a deliberately suppressed wake is recovered by the safety net within
its period; FIFO order of two waiters on one socket; UDP recv. Run the full suite from brief 13 with the
flag both unset and set (`TS_EXPERIMENT_TARGETED_REPLAY=1 cargo test ...`), and `ts_forwarder --test
forwarding` several times in both modes.

Write `docs/codex-14-targeted-replay.md`: commits, design deviations, tests, risks, expected counter values.
