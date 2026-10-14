# TCP sender recovery — Brief 24

2026-10-14 (Copenhagen). Isolated worktree `/Volumes/git/ts-exit/scratch/wt-lossbench`, branch `ts-exit/tcp-recovery`. Base sender: `ac68a55`; final code: `3ce8908` (later commits contain results/docs only).

## Confirmed mechanism and changes

The sender ignored SACK, rewound `remote_last_seq` on fast retransmit and resent the window. It lost the original recovery boundary and had no partial-ACK handling. Cwnd limited each packet rather than the flight. Cubic also overwrote slow start and applied its curve in bytes instead of segments.

Address-free wire trace, 25 ms / 150 Mbit/s / 64 KiB queue / 2 MiB buffer:

```text
94.036 ms  sender seq=1630930476 len=1240 accepted
111.756 ms peer ACK=1630919316 SACK=[1630930476,1630931716)
142.057 ms sender seq=1630930476 len=1240 accepted again
212.246 ms sender seq=1630930476 len=1240 accepted again
```

Another segment's retransmissions at 324.902 and 1326.119 ms show a 1001 ms RTO gap. The async runner had a separate clock bug: an I/O future retained its timestamp across Pending, producing 0–20 ms RTT samples on this 25 ms path. Its regression records 0 ms on the original runner and 42 ms on the fix after a 42 ms suspension.

Separate commits implement:

- [NewReno partial-ACK repair](https://www.rfc-editor.org/rfc/rfc6582), original flight boundaries, and a bounded 32-range [SACK/Pipe recovery](https://www.rfc-editor.org/rfc/rfc6675). Only cumulative ACK frees data; RTO clears advisory SACK state. Rescue/RACK are not implemented.
- Flight-wide cwnd enforcement; corrected Reno and Cubic, including [Cubic's Reno-friendly region](https://www.rfc-editor.org/rfc/rfc9438), once-per-episode decrease and a bounded slow-start restart after RTO.
- Fresh async timestamps, recovery-safe RTT sampling, pacing at 1.2*cwnd/srtt with bounded bursts, and a 200 ms computed RTO floor. ACKs remain immediate; initial handshake RTO stays one second; RTT variance/backoff remain. Minimum observed RTT ≤5 ms bypasses pacing. Smoltcp's pacing/RTO API defaults remain intact; the netstack enables this policy.

Reno reaches 108.6/126.4 Mbit/s (1/8 flows, 64 KiB queue), and 142.1 with eight flows at 256 KiB. Cubic is selected for the better shallow-queue single download; Reno wins the large-queue eight-flow check. Details and tests are in `vendor/smoltcp/PATCHES.md`.

Both bench stacks and production RX use 32 assembler holes, also helping uploads. A 256-hole control leaves difficult eight-flow rates essentially unchanged: 84.8→87.4 (64 KiB down), 87.3→86.0 (256 KiB down), 82.1→83.9 (128 KiB upload sink). The bench is not limited by the original four-hole receiver.

## Before / after

MTU 1280, symmetric 25 ms RTT, 150 Mbit/s per direction, tail-drop queue, no random loss; 15 s including startup. Same instrumented harness/32-hole receivers for both senders. Sequential runs, no concurrent builds, four build jobs/workers maximum. `nice -n 10` was invoked; the sandbox rejects the priority change.

Goodput: Mbit/s, measured at the real sink for uploads. Pause: longest application I/O gap anywhere in the run. Probe: fresh TCP connect + one-byte echo every 200 ms, excluding deadline cancellations. It is a synthetic loaded-latency check, not the laptop TLS/HTTP test; stalled bulk flows can leave its link idle.

### Downloads

| Queue KiB | Buffer KiB | Flows | Goodput before → after | Pause s before → after | Probe p95 ms before → after | Probe max ms after |
|---:|---:|---:|---:|---:|---:|---:|
| 64 | 256 | 1 | 2.7 → 70.2 | 4.06 → 0.03 | 63.3 → 57.8 | 61.0 |
| 64 | 256 | 8 | 13.0 → 123.2 | 6.86 → 0.25 | 63.1 → 61.0 | 61.3 |
| 64 | 2048 | 1 | 0.2 → 125.8 | 7.76 → 0.03 | 62.8 → 59.7 | 62.8 |
| 64 | 2048 | 8 | 1.2 → 123.8 | 11.67 → 0.23 | 61.8 → 62.0 | 67.8 |
| 128 | 256 | 1 | 24.6 → 70.2 | 1.03 → 0.03 | 66.3 → 57.0 | 58.4 |
| 128 | 256 | 8 | 78.9 → 130.2 | 6.50 → 0.23 | 66.9 → 66.8 | 67.5 |
| 128 | 2048 | 1 | 1.8 → 138.0 | 6.95 → 0.03 | 63.4 → 66.5 | 67.7 |
| 128 | 2048 | 8 | 4.9 → 128.3 | 11.80 → 0.23 | 66.5 → 65.8 | 67.7 |
| 256 | 256 | 1 | 73.1 → 70.2 | 0.03 → 0.03 | 58.9 → 57.6 | 61.4 |
| 256 | 256 | 8 | 117.1 → 137.5 | 4.06 → 0.23 | 80.0 → 79.9 | 81.5 |
| 256 | 2048 | 1 | 3.6 → 142.4 | 7.06 → 0.04 | 63.1 → 80.8 | 80.9 |
| 256 | 2048 | 8 | 14.0 → 128.1 | 11.36 → 0.23 | 82.1 → 80.8 | 82.3 |

### Uploads, 2 MiB buffer

| Queue KiB | Flows | Sink goodput before → after | Pause s before → after | Probe p95 ms before → after | Probe max ms after |
|---:|---:|---:|---:|---:|---:|
| 64 | 1 | 0.3 → 132.7 | 7.58 → 0.03 | 62.5 → 61.1 | 61.7 |
| 64 | 8 | 2.2 → 125.6 | 11.64 → 0.63 | 63.6 → 60.8 | 61.3 |
| 128 | 1 | 2.1 → 139.0 | 6.69 → 0.03 | 63.8 → 67.1 | 67.1 |
| 128 | 8 | 8.3 → 129.7 | 10.94 → 0.23 | 72.3 → 66.8 | 70.8 |
| 256 | 1 | 3.0 → 142.5 | 7.15 → 0.04 | 63.1 → 80.7 | 81.0 |
| 256 | 8 | 24.7 → 129.3 | 7.08 → 0.24 | 91.8 → 81.0 | 83.4 |

A single 256 KiB flow remains window/RTT capped; that window cannot fill this link at 25 ms plus ACK/stack delay.

### No-loss regression, 30-second runs

| RTT ms | Buffer KiB | Flows | Direction | Before | After | Δ |
|---:|---:|---:|---|---:|---:|---:|
| 0 | 256 | 1 | up | 15418.6 | 16676.4 | +8.2% |
| 0 | 256 | 1 | down | 14894.2 | 16183.7 | +8.7% |
| 0 | 256 | 8 | up | 19236.8 | 22313.7 | +16.0% |
| 0 | 256 | 8 | down | 16352.8 | 17801.7 | +8.9% |
| 40 | 4096 | 1 | up | 662.9 | 719.5 | +8.5% |
| 40 | 4096 | 1 | down | 665.9 | 719.1 | +8.0% |
| 40 | 4096 | 8 | up | 5071.5 | 5730.4 | +13.0% |
| 40 | 4096 | 8 | down | 5679.1 | 5936.8 | +4.5% |

No no-loss regression: all eight rows improve. The bounded eight-flow check (`VERIFY_MB=16`, queue 64 KiB, buffer 2 MiB) completes 128 MiB in 10.07 s against a 30 s deadline.

## Validation / reproduction

Passed: 685 smoltcp library tests with default protocols + Reno/Cubic/32 holes; 408 IP/TCP tests without congestion features; `no_std` + defmt check; forwarder and all three netstack crates’ unit/integration/doctests (Tokio + futures-io). Clippy with `-D warnings` for vendor library/tests and all targets of those four workspace packages, and workspace/vendor fmt checks are clean. Raw records: `tcp-recovery-24-results.json`; trace evidence: `tcp-recovery-24-trace.txt` alongside the repository copy. The async clock regression fails on the original runner and passes on the fix.

```sh
cargo +1.95.0 bench --locked -j 4 -p geiserx_ts_forwarder --bench forward_throughput --no-run
RTT_MS=25 BW_MBPS=150 QUEUE_KB=64 TCP_BUF_KB=2048 LATENCY=1 nice -n 10 <binary> 15 1 down
# Repeat queues 64/128/256, buffers 256/2048, flows 1/8; up uses 2048.
# No loss: BW_MBPS=0 RTT_MS=0 TCP_BUF_KB=256 or RTT_MS=40 TCP_BUF_KB=4096; 30 seconds.
```

## Buffer recommendation

Recommend **1 MiB per direction** (`tcp_buffer_size = Some(1024 * 1024)`) for this ~150 Mbit/s / 25 ms exit-node workload. Additional 1 MiB checks reach 123–142 Mbit/s (64/128/256 KiB queues, including eight flows at 64 KiB), with 0.03–0.23 s pauses and probe p95 61–80 ms, matching 2 MiB. The BDP is about 458 KiB; 256 KiB remains single-flow capped. One MiB allocates ~2 MiB/socket eagerly (roughly 2 GiB for 1,000 flows); retain 256 KiB on memory-constrained, high-concurrency nodes. This branch retains the current 256 KiB allocation default; set the deployment override to 1 MiB. Two MiB does not buy throughput at this RTT.

After recovery works, the larger window adds at most ~23 ms to the single-flow probe p95 at a 256 KiB queue because it fills the link/queue; eight-flow p95 is essentially unchanged across buffer sizes. There are no multi-second pauses in the final matrix (largest download 0.25 s; upload 0.63 s).

No live-node or remote changes. Real Wi-Fi TLS/HTTP confirmation is outside this offline run.
