# Pacing alone on stock smoltcp 0.14.0 — Brief 32

2026-10-15. **Pacing alone improves the median across the 24 download cell medians:
Reno 51.2 → 112.1 Mbit/s; Cubic 51.0 → 98.9.** The worst download pause stays
**2.03 s** in both variants. Gains depend on the queue/controller; regressions
are included below. This is separate pristine-stock and pacing-only binaries,
with window, CC and all other settings held fixed within each A/B pair.

## Setup

Branch `ts-exit/pacing-ab`, worktree `scratch/pacing-ab`, starts at requested
`0e10bc3` (GeiserX main + serve, workspace 0.57.6), with bench `9af3f91` and
pristine crates.io **smoltcp 0.14.0** import `038876f` cherry-picked. The stock
binary is frozen at `fc6fca1`, before adding pacing. Both binaries use Rust
**1.95.0**, the same release profile/Cargo.lock/features, stock Reno or Cubic,
`std` (for unchanged Cubic floating-point methods), and assembler capacity 32.
Host: macOS 26.6.2/arm64, 10 logical CPUs; timed runs 21:26–23:28 Copenhagen.
`[patch.crates-io]` pins `vendor/smoltcp`; hashes are in the setup record.

B adds only the opt-in pacer from `21e8417` + `92c7b2e`, adapted to stock:
1.2 × cwnd/SRTT, max(cwnd/8, 2 MSS) burst, capped at 32 MSS after loss, with
debt capped at one SRTT. No valid RTT or baseline RTT ≤5 ms bypasses pacing.
ACKs and all retransmissions bypass the budget; stock RTTE’s high-water sequence
identifies RTO replay. Only stock fast-retransmit/RTO signals mark pacing loss.
**No NewReno, sender SACK, B1/B2 fixes, Cubic changes or RTO backport.** Computed
minimum and initial RTO remain **1 s**. `BENCH_CC=reno|cubic` selects both ends;
`BENCH_PACING=0|1` controls B. A uses the separate pristine binary.

W is `TCP_BUF_KB`: fixed RX and TX allocation per socket at both ends.
MTU 1280, Nagle disabled as in GeiserX, `FWD_MODE=ports`, four Tokio workers,
`LOSS_PCT=0`, `LATENCY=1`. The BW limiter and `QUEUE_KB × 1024` byte tail queue apply in each
relay direction and are shared by all flows. RTT is symmetric propagation
(RTT/2 each way). **Three repetitions per variant/cell/controller**, adjacent
A/B/A/B/A/B; controller order reverses between cells. All runs are sequential,
15 s shaped and 30 s unshaped, including bulk startup. The initial cold-connect
check is outside the measurement; there is no bulk warm-up. Some shaped runs have
zero drops. No-loss controls disable the bandwidth limiter and cover downloads.

**Goodput:** aggregate received application bytes/s, Mbit/s; uploads use the
loopback server’s `server_rx_mbps`, not bytes accepted into the sending buffer.
**Pause:** worst gap between successful application reads (down) or accepted
writes (up), on any individual bulk flow, including its trailing gap to the
measurement deadline. It is not an all-flows-at-once stall; upload pauses can
be masked by buffering. **Drops:** discarded IP packets at the two relay tail
queues, summed, including ACK/probe traffic; counts, not a drop rate.
**Loaded probe:** a fresh TCP connect plus one-byte echo every ~200 ms; p95
includes both handshake and echo, not a ping RTT.

Validation: pristine library **697 tests**; patched library **705**, both with
pacing disabled and with all TCP test fixtures opted in. Netstack/forwarder
tests, workspace clippy `-D warnings`, workspace fmt and touched-file fmt pass.
A separate five-second instrumented smoke run counted Reno **0 → 11,782** and
Cubic **0 → 9,857** paced deferrals (off → on). Matrix binaries have no counter.

## Summary for the issue

Goodput is Mbit/s; arrows are **stock → stock + pacer**. The aggregate is
the unweighted median of the cell medians, separately for each controller.

| Scope | Reno | Cubic |
|---|---:|---:|
| Bottleneck matrix: median across 32 cells | 51.2 → 112.8 | 51.0 → 98.9 |
| Bottleneck downloads: median across 24 cells | 51.2 → 112.1 | 51.0 → 98.9 |
| Bottleneck uploads: median across 8 cells | 61.1 → 121.0 | 55.8 → 111.7 |
| Down B150/Q64/W1024/F1 | 13.0 → 26.2 | 12.6 → 47.1 |
| Down B150/Q64/W1024/F8 | 26.8 → 99.3 | 27.7 → 92.3 |
| Down B150/Q256/W1024/F8 | 114.4 → 116.9 | 109.4 → 101.6 |
| Down B250/Q64/W1024/F1 | 8.9 → 103.6 | 9.9 → 151.6 |
| Down B250/Q64/W1024/F8 | 25.1 → 126.6 | 26.3 → 125.2 |
| Down B250/Q256/W1024/F8 | 148.8 → 170.0 | 115.9 → 147.5 |
| No loss RTT0/W256/F1 | 9362.5 → 9440.2 | 9516.4 → 9574.6 |
| No loss RTT0/W256/F8 | 23546.3 → 23518.3 | 23640.4 → 23579.2 |
| No loss RTT40/W4096/F1 | 458.1 → 679.6 | 505.7 → 678.3 |
| No loss RTT40/W4096/F8 | 5144.4 → 5787.0 | 4808.2 → 5785.2 |

Across all 32 shaped cells, cell medians improve in **30/32 Reno** and **26/32
Cubic** comparisons. The two Cubic regressions over 10% are down
B150/Q128/W1024/F1 (**29.1 → 24.8**, −14.8%) and up B150/Q256/W1024/F8
(**112.9 → 97.6**, −13.6%). Some paced cells have longer pauses or more total
drops despite higher goodput.

RTT-0 control median changes span **−0.26% to +0.83%**, with overlapping run
ranges. RTT-40 control medians rise **12.5–48.4%**; every control has zero tail
drops. Unshaped controls retain the software and receiver-window limits.
All **28,294 loaded probes** succeed across **432 runs**.

## Full matrix

B = Mbit/s per direction, Q/W = KiB, F = flow count. All cells have three
repetitions per variant. Goodput: median [min–max]. Pause: longest observed
across all three runs. Drops: median packet count. Probe: worst run’s loaded
probe p95. Paired columns use **stock / pacer**. Raw samples retain every
measurement, including probe failures.

### Downloads, RTT 25 ms

| B/Q/W/F (or RTT/W/F) | CC | Stock Mbit/s [range] | Paced Mbit/s [range] | Pause s | Drops | Probe p95 ms |
|---|---|---:|---:|---:|---:|---:|
| 150/64/1024/1 | reno | 13.0 [12.5–15.4] | 26.2 [25.5–100.8] | 1.03 / 1.03 | 30 / 20 | 64.0 / 60.0 |
| 150/64/1024/1 | cubic | 12.6 [9.6–15.1] | 47.1 [43.0–103.2] | 1.03 / 1.03 | 21 / 119 | 63.5 / 63.3 |
| 150/64/1024/8 | reno | 26.8 [25.6–27.0] | 99.3 [98.0–104.7] | 1.03 / 2.03 | 214 / 252 | 64.0 / 63.5 |
| 150/64/1024/8 | cubic | 27.7 [27.0–28.0] | 92.3 [85.6–93.1] | 1.03 / 2.03 | 287 / 410 | 63.4 / 63.1 |
| 150/64/256/1 | reno | 11.8 [10.5–12.0] | 69.3 [69.0–69.4] | 1.04 / 0.03 | 8 / 0 | 63.3 / 59.5 |
| 150/64/256/1 | cubic | 18.8 [12.4–26.4] | 69.4 [69.2–69.4] | 1.03 / 0.03 | 6 / 0 | 63.7 / 58.6 |
| 150/64/256/8 | reno | 26.2 [25.9–26.6] | 107.9 [98.5–109.1] | 1.03 / 2.03 | 217 / 218 | 63.1 / 62.3 |
| 150/64/256/8 | cubic | 26.8 [26.5–31.9] | 98.6 [97.9–103.3] | 1.03 / 1.03 | 286 / 281 | 63.8 / 62.3 |
| 150/128/1024/1 | reno | 97.2 [49.7–99.2] | 111.5 [110.6–111.7] | 2.03 / 1.03 | 30 / 23 | 63.9 / 62.5 |
| 150/128/1024/1 | cubic | 29.1 [29.1–66.9] | 24.8 [24.8–25.7] | 1.03 / 1.03 | 116 / 82 | 65.3 / 63.9 |
| 150/128/1024/8 | reno | 115.1 [105.4–115.5] | 112.4 [112.0–113.1] | 1.03 / 1.03 | 214 / 278 | 66.1 / 66.5 |
| 150/128/1024/8 | cubic | 106.4 [96.8–114.5] | 96.4 [77.5–102.8] | 2.03 / 1.03 | 418 / 465 | 66.6 / 67.2 |
| 150/128/256/1 | reno | 65.0 [54.3–66.0] | 69.3 [69.2–69.4] | 0.03 / 0.03 | 0 / 0 | 62.8 / 58.1 |
| 150/128/256/1 | cubic | 64.7 [64.6–65.2] | 69.2 [69.0–69.3] | 0.03 / 0.06 | 0 / 0 | 60.5 / 57.9 |
| 150/128/256/8 | reno | 115.6 [111.8–116.7] | 111.7 [111.2–114.2] | 1.03 / 1.03 | 157 / 172 | 66.4 / 66.2 |
| 150/128/256/8 | cubic | 109.3 [108.4–109.5] | 112.7 [112.7–116.0] | 2.03 / 1.03 | 315 / 261 | 65.7 / 67.0 |
| 150/256/1024/1 | reno | 13.7 [13.5–109.8] | 123.8 [120.7–124.5] | 1.03 / 1.03 | 39 / 32 | 63.5 / 64.1 |
| 150/256/1024/1 | cubic | 85.7 [35.3–89.1] | 91.3 [38.6–110.1] | 1.03 / 1.03 | 43 / 40 | 80.7 / 79.3 |
| 150/256/1024/8 | reno | 114.4 [112.9–117.5] | 116.9 [116.6–117.0] | 1.04 / 1.04 | 235 / 245 | 80.5 / 79.9 |
| 150/256/1024/8 | cubic | 109.4 [101.2–113.3] | 101.6 [91.3–105.3] | 1.39 / 1.04 | 386 / 428 | 79.5 / 79.2 |
| 150/256/256/1 | reno | 65.0 [64.4–65.4] | 69.3 [69.3–69.4] | 0.03 / 0.03 | 0 / 0 | 62.3 / 58.3 |
| 150/256/256/1 | cubic | 65.0 [64.7–65.1] | 69.4 [69.3–69.4] | 0.03 / 0.03 | 0 / 0 | 63.3 / 58.0 |
| 150/256/256/8 | reno | 112.5 [111.9–115.2] | 118.0 [115.2–124.0] | 1.04 / 1.03 | 212 / 247 | 79.5 / 80.1 |
| 150/256/256/8 | cubic | 106.5 [87.7–110.5] | 99.2 [98.6–111.1] | 1.04 / 1.04 | 414 / 398 | 80.8 / 80.1 |
| 250/64/1024/1 | reno | 8.9 [7.5–11.7] | 103.6 [96.7–110.8] | 1.03 / 1.03 | 4 / 6 | 63.3 / 61.9 |
| 250/64/1024/1 | cubic | 9.9 [9.0–11.7] | 151.6 [120.6–178.3] | 1.03 / 1.03 | 11 / 30 | 63.7 / 62.5 |
| 250/64/1024/8 | reno | 25.1 [21.5–26.3] | 126.6 [120.8–128.1] | 1.04 / 1.03 | 136 / 253 | 62.6 / 62.4 |
| 250/64/1024/8 | cubic | 26.3 [23.8–26.9] | 125.2 [118.9–130.2] | 1.04 / 2.03 | 150 / 286 | 62.4 / 62.6 |
| 250/64/256/1 | reno | 11.2 [8.8–16.7] | 69.3 [69.3–69.3] | 0.06 / 0.03 | 2 / 0 | 63.2 / 58.1 |
| 250/64/256/1 | cubic | 14.1 [9.8–14.5] | 69.2 [69.0–69.3] | 1.03 / 0.03 | 5 / 0 | 63.2 / 57.9 |
| 250/64/256/8 | reno | 26.5 [25.2–28.1] | 113.1 [100.7–120.0] | 1.03 / 2.03 | 160 / 256 | 62.8 / 62.5 |
| 250/64/256/8 | cubic | 26.1 [20.9–27.8] | 132.0 [112.8–144.9] | 1.03 / 2.03 | 189 / 292 | 62.5 / 61.9 |
| 250/128/1024/1 | reno | 22.6 [17.0–26.7] | 131.7 [131.5–131.8] | 1.03 / 1.03 | 4 / 27 | 63.2 / 62.2 |
| 250/128/1024/1 | cubic | 18.9 [16.1–20.5] | 152.0 [40.0–157.8] | 1.03 / 1.03 | 25 / 38 | 63.8 / 62.1 |
| 250/128/1024/8 | reno | 49.1 [48.6–54.5] | 165.8 [148.2–166.2] | 1.04 / 2.03 | 161 / 287 | 64.1 / 62.5 |
| 250/128/1024/8 | cubic | 50.0 [46.7–52.1] | 170.0 [148.1–205.9] | 1.04 / 1.03 | 266 / 310 | 64.2 / 62.7 |
| 250/128/256/1 | reno | 21.4 [20.4–29.6] | 69.3 [69.2–69.3] | 1.03 / 0.03 | 3 / 0 | 62.9 / 58.5 |
| 250/128/256/1 | cubic | 20.3 [17.8–20.6] | 69.2 [69.2–69.3] | 1.04 / 0.03 | 21 / 0 | 63.7 / 57.9 |
| 250/128/256/8 | reno | 53.2 [44.9–58.0] | 166.8 [164.8–176.3] | 1.03 / 1.03 | 147 / 233 | 63.5 / 62.0 |
| 250/128/256/8 | cubic | 51.9 [48.9–86.6] | 159.5 [158.1–185.3] | 1.03 / 1.03 | 268 / 306 | 64.2 / 62.0 |
| 250/256/1024/1 | reno | 134.3 [86.8–159.3] | 153.1 [138.4–158.4] | 1.03 / 1.03 | 6 / 2 | 68.6 / 67.2 |
| 250/256/1024/1 | cubic | 143.7 [61.5–207.6] | 202.3 [195.0–225.8] | 2.03 / 1.03 | 3 / 2 | 68.6 / 68.7 |
| 250/256/1024/8 | reno | 148.8 [140.3–190.3] | 170.0 [163.4–170.7] | 1.03 / 1.03 | 238 / 294 | 69.5 / 67.9 |
| 250/256/1024/8 | cubic | 115.9 [98.4–151.7] | 147.5 [146.2–154.1] | 2.03 / 1.03 | 474 / 411 | 67.0 / 68.2 |
| 250/256/256/1 | reno | 57.4 [57.4–58.6] | 69.3 [69.3–69.3] | 0.03 / 0.03 | 0 / 0 | 64.6 / 57.6 |
| 250/256/256/1 | cubic | 59.8 [59.7–60.9] | 69.2 [69.1–69.4] | 0.03 / 0.03 | 0 / 0 | 64.2 / 58.7 |
| 250/256/256/8 | reno | 137.8 [98.5–198.4] | 171.6 [169.9–179.5] | 1.03 / 1.03 | 224 / 254 | 68.4 / 68.1 |
| 250/256/256/8 | cubic | 117.7 [92.2–172.7] | 162.4 [147.3–185.6] | 1.03 / 1.03 | 376 / 271 | 67.5 / 67.7 |

### Upload subset, RTT 25 ms

| B/Q/W/F (or RTT/W/F) | CC | Stock Mbit/s [range] | Paced Mbit/s [range] | Pause s | Drops | Probe p95 ms |
|---|---|---:|---:|---:|---:|---:|
| 150/64/1024/1 | reno | 12.2 [11.3–15.5] | 91.5 [76.0–92.9] | 1.03 / 1.03 | 5 / 8 | 63.2 / 62.6 |
| 150/64/1024/1 | cubic | 16.8 [12.4–21.5] | 23.0 [22.5–68.3] | 1.03 / 1.04 | 4 / 129 | 62.9 / 63.5 |
| 150/64/1024/8 | reno | 27.0 [26.3–27.2] | 102.6 [95.9–107.1] | 1.04 / 2.03 | 231 / 267 | 62.7 / 63.1 |
| 150/64/1024/8 | cubic | 27.7 [26.1–28.4] | 97.6 [82.4–110.5] | 1.45 / 2.03 | 286 / 335 | 62.7 / 63.2 |
| 150/256/1024/1 | reno | 94.5 [13.1–108.3] | 124.1 [122.7–124.8] | 1.03 / 1.03 | 31 / 30 | 62.7 / 63.3 |
| 150/256/1024/1 | cubic | 83.9 [34.4–84.9] | 76.3 [39.8–94.0] | 1.03 / 1.04 | 42 / 48 | 81.4 / 81.2 |
| 150/256/1024/8 | reno | 111.4 [108.1–123.0] | 117.9 [116.2–120.9] | 1.04 / 1.04 | 256 / 302 | 79.7 / 80.6 |
| 150/256/1024/8 | cubic | 112.9 [109.2–118.7] | 97.6 [95.5–106.6] | 2.03 / 1.04 | 341 / 392 | 80.3 / 81.1 |
| 250/64/1024/1 | reno | 8.8 [7.9–11.8] | 84.1 [57.9–115.4] | 1.03 / 1.03 | 3 / 4 | 62.9 / 60.6 |
| 250/64/1024/1 | cubic | 15.3 [14.4–16.9] | 145.6 [143.6–146.8] | 1.04 / 1.03 | 43 / 15 | 63.1 / 63.0 |
| 250/64/1024/8 | reno | 27.7 [26.4–28.5] | 124.1 [123.4–136.1] | 1.03 / 2.03 | 124 / 222 | 63.0 / 64.0 |
| 250/64/1024/8 | cubic | 27.1 [26.3–27.3] | 125.7 [106.5–125.8] | 1.03 / 2.03 | 163 / 289 | 63.2 / 63.0 |
| 250/256/1024/1 | reno | 201.6 [177.2–203.7] | 230.8 [170.0–230.9] | 1.03 / 1.03 | 0 / 0 | 69.5 / 69.7 |
| 250/256/1024/1 | cubic | 198.9 [83.8–202.3] | 230.7 [223.7–231.1] | 1.03 / 0.03 | 0 / 0 | 69.0 / 69.8 |
| 250/256/1024/8 | reno | 142.6 [138.9–148.6] | 181.2 [181.1–182.8] | 1.03 / 1.03 | 231 / 235 | 68.6 / 69.5 |
| 250/256/1024/8 | cubic | 147.3 [142.6–170.2] | 154.3 [115.2–186.6] | 1.03 / 1.03 | 295 / 348 | 67.9 / 68.5 |

### No-loss download controls; no bandwidth limiter

| B/Q/W/F (or RTT/W/F) | CC | Stock Mbit/s [range] | Paced Mbit/s [range] | Pause s | Drops | Probe p95 ms |
|---|---|---:|---:|---:|---:|---:|
| 0/256/1 | reno | 9362.5 [8963.6–9590.0] | 9440.2 [8926.0–9527.1] | 0.00 / 0.00 | 0 / 0 | 0.5 / 0.7 |
| 0/256/1 | cubic | 9516.4 [9455.3–9572.5] | 9574.6 [9207.8–9679.0] | 0.00 / 0.00 | 0 / 0 | 0.6 / 0.6 |
| 0/256/8 | reno | 23546.3 [21514.7–23753.7] | 23518.3 [21385.5–23660.6] | 0.01 / 0.00 | 0 / 0 | 2.9 / 3.0 |
| 0/256/8 | cubic | 23640.4 [21513.5–23658.3] | 23579.2 [20915.7–23621.1] | 0.00 / 0.00 | 0 / 0 | 2.7 / 3.1 |
| 40/4096/1 | reno | 458.1 [361.0–534.5] | 679.6 [679.1–681.8] | 0.05 / 0.05 | 0 / 0 | 94.2 / 90.2 |
| 40/4096/1 | cubic | 505.7 [496.3–547.3] | 678.3 [676.5–680.0] | 0.05 / 0.05 | 0 / 0 | 94.2 / 90.4 |
| 40/4096/8 | reno | 5144.4 [4832.7–5415.8] | 5787.0 [5786.1–5797.4] | 0.05 / 0.05 | 0 / 0 | 94.2 / 89.9 |
| 40/4096/8 | cubic | 4808.2 [4733.7–5261.7] | 5785.2 [5778.5–5842.1] | 0.05 / 0.05 | 0 / 0 | 94.5 / 90.4 |

## Caveats and reproducibility

Both synthetic overlay endpoints use the selected smoltcp build/controller;
the egress leg is host OS TCP over loopback. **No WireGuard, TLS or real client
link; one machine.** The RTT-0 controls are CPU/scheduler limited. Three repeats
show the observed range, not a confidence interval, and the aggregate weights
each matrix cell equally. Drop counts can rise when more packets are sent.
Stock recovery and its one-second RTO stalls remain; these numbers isolate
pacing and cannot be attributed to the Brief 30 recovery patch set.

All commands invoke `nice -n 10`, but the sandbox rejects `setpriority`; priority
could not actually be lowered. Builds use at most four jobs and finish before
timed benchmarks. Main, live node and remotes are unchanged; nothing is submitted
upstream. Reproduction uses the recorded binary hashes and sequential runner.

Branch source commits: `d4a732a` (bench cherry-pick), `131c390` (pristine vendor
cherry-pick), `fc6fca1` (common CC/std switch; frozen stock), `1407fb5` (pacer),
`0147c39` (bench pacing switch, runner and frozen-binary metadata).
To rebuild: use the setup record’s command at `fc6fca1` for A and `0147c39` for B,
copy the resulting bench executable, then run the matrix script (fixed paths
are recorded there). The report renderer reproduces this document from the
raw data and setup record; long build/test logs remain in `scratch/pacing32-*.log`.

[Raw results](../scratch/pacing-ab/docs/pacing-ab-32-results.jsonl), [binary/source setup](../scratch/pacing-ab/docs/pacing-ab-32-setup.json),
[matrix runner](../scratch/pacing-ab/docs/pacing-ab-32-matrix.py), [report renderer](../scratch/pacing-ab/docs/pacing-ab-32-report.py).

Report/artifacts commit: `1527c63a58a49b83c3a57d7a68fe3766bc0b79d1` on `ts-exit/pacing-ab`.
