# Brief 33 — live pacing instrumentation

Branch `ts-exit/pacing-live`, worktree `scratch/pacing-live`, from
`ts-exit/pacing-ab` **1527c63**. Code commits:

- **1a5d548** — smoltcp counters and regression tests.
- **cd65745** — atomic socket-creation policy; enable stock Cubic/std so the
  standalone example supports the inherited `BENCH_CC` switch.
- **0b1954b** — exit-node file controls and atomic JSON snapshots.

Still smoltcp **0.14.0 + pacing only**, with stock recovery and 1 s minimum
RTO. Reno remains the default. No live-node/daemon, main or remote changes.

Binary: `/Volumes/git/ts-exit/bin/exit_node-pacing-live` (native macOS arm64,
Rust 1.95.0). SHA-256: **c6517678b946a3441a1660fc3d5d0761c0fc6e0386a0c19de36d722ba23a0094**.
Built with `cargo +1.95.0 build --offline -j 4 --profile exitnode --features ring-aead --example exit_node`. `--help` retains `--serve-tcp`
and `--tcp-buffer-size`.

## Control and snapshots

- `BENCH_PACING=1`: initial pacing policy; otherwise off. Read once per process.
- `BENCH_CC=cubic`: select stock Cubic; otherwise Reno (unchanged).
- `TS_PACING_CONTROL_FILE=/path/control`: read initially before socket creation,
  then once per second. Trimmed, case-sensitive `on` / `off`; missing means off,
  overriding the initial env value. Invalid content/other read failures warn and
  retain the policy. Changes log at info.
- `TS_TCP_STATS_FILE=/path/stats.json`: initially and once per second write one
  JSON object to `<path>.<pid>.tmp`, then rename over the destination. Parent
  directory must exist. Write failures warn and polling continues.

```json
{"ts_ms":0,"pacing":false,"data_segs":0,"retx_segs":0,"fast_retx":0,"rto":0,"paced_deferrals":0}
```

`ts_ms` is Unix milliseconds. `pacing` is the policy for **new sockets**, not the
state of every current connection; true can still bypass pacing without a valid
RTT or at baseline RTT ≤5 ms. Existing sockets, including already-listening
sockets, retain their setting. After each toggle, wait for the snapshot, make a
throwaway connection to the same destination port (to replace its waiting
listener), then open the measured connection. Disable HTTP connection reuse and
hold controller/buffer size constant across trials.

## Counter definitions

All counters are cumulative since process start, process-wide across smoltcp TCP
sockets, in either direction; relaxed atomic snapshots are not transactional.
Use interval differences; background traffic contributes. Counts describe local
emission/recovery, **not measured wire drops** or WireGuard/host TCP traffic.

| Field | Exact increment |
|---|---|
| `data_segs` | Successful device emit with payload >0, including retransmissions, keepalives and zero-window probes; empty ACK/SYN/FIN excluded. |
| `retx_segs` | Such a segment starts below that socket's prior highest successfully emitted sequence end (wrap-aware; never rewound for recovery, cleared on socket reset). Partial overlaps count once; keepalives/probes can count. Failed emits count neither segment counter. |
| `fast_retx` | Dispatch handles the stock fast-retransmit signal. An event counts even if the subsequent emit fails. |
| `rto` | Dispatch handles an expired retransmission timer, including SYN/FIN retries and failed emits; excludes keepalive/zero-window-probe timers. |
| `paced_deferrals` | A dispatch finds buffered data eligible under stock window/Nagle checks but pacing allowance is zero, including attempts that send an ACK instead. Repeated attempts count separately; `poll_at` queries do not count. |

## Checks

Passed: smoltcp library **706 tests each**, pacing off/on; netstack/core/socket
and forwarder **189 tests**; exit-node **2 tests**; workspace/all-target clippy
`-D warnings` with `ring-aead`; workspace fmt and touched vendor-file fmt;
smoltcp no_std/Reno check. Tests cover policy retention, file handling/JSON
replacement, pacing deferrals, fast/RTO recovery, failed emit and sequence wrap.
Logs: `scratch/pacing-live-33-results/`. All builds capped at four jobs;
`nice -n 10` was attempted but the sandbox rejects `setpriority`.
