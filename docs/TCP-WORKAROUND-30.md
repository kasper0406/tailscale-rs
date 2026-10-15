# TCP workaround — Brief 30

2026-10-14–15, Copenhagen. **Recommend patched 0.14.0 + Cubic, sender SACK,
bounded pacing and a 200 ms computed RTO floor; run 1 MiB per direction.**
The handshake RTO remains 1 s and the allocation default remains 256 KiB.
Worktree: `scratch/tcp-0.14`, branch `ts-exit/tcp-0.14`, base `0e10bc3`.
Brief 30 HEAD: `cb547f7` (report/artifacts commit); current 30b HEAD: `92c7b2e850f0a5dbff10ac129852bfdab01c2416`.
No live node, daemon, main branch or remote changes.

Stock-library mitigations do not meet the larger-window/loss-recovery goal.
At 1 MiB, final single downloads reach **121–139 Mbit/s at 150** and
**149–230 Mbit/s at 250**, versus ~69 Mbit/s with 256 KiB. Final bulk pauses
are at most **0.23 s**, versus stock's **2.03 s**. All loaded probes succeed;
final loaded-probe p95 is at most **81.9 ms**.

## Matrix

MTU 1280; RTT 25 ms; symmetric bandwidth and tail-drop queues; no random loss.
15 s per lossy row, 30 s per no-loss row, including startup; one run per cell.
Both synthetic TCP stacks share the selected policy. Upload goodput is measured
at the native loopback sink; pause is the longest bulk application I/O gap
(sender-side for upload). Probe is fresh TCP connect + one-byte echo every
200 ms, with end-of-run cancellations excluded. This bypasses WireGuard and
TLS; upload's synthetic sender is smoltcp, not the laptop's kernel TCP.

**Cell = goodput Mbit/s / longest pause s / loaded-probe p95 ms.**
Burst8 is Reno with cached device `max_burst_size=Some(8)`; KA1s is Reno with
1 s keepalive. Recovery Reno has B1/B2, min RTO, NewReno and SACK, without
pacing. Paced Reno adds pacing; Final Cubic changes only the controller.
Full records: [JSONL](../scratch/tcp-0.14/docs/tcp-workaround-30-results.jsonl).

| Variant | Lossy median Mbit/s | Maximum pause s | Maximum probe p95 ms |
|---|---:|---:|---:|
| Stock Reno | 57.4 | 2.03 | 80.9 |
| Burst8 | 27.8 | 1.04 | 80.2 |
| Stock Cubic | 58.0 | 2.03 | 80.9 |
| KA1s | 50.5 | 2.03 | 80.2 |
| Recovery Reno | 61.5 | 0.23 | 80.6 |
| Paced Reno | 130.2 | 0.04 | 80.2 |
| Final Cubic | 140.9 | 0.23 | 81.9 |

### 150 Mbit/s, RTT 25 ms

| Q/W KiB; flows; dir | Stock | Burst8 | Cubic | KA1s | Recovery R | Paced R | Final C |
|---|---|---|---|---|---|---|---|
| 64/256; 1; down | 17.8/1.03/62.4 | 20.7/0.03/62.8 | 12.7/1.03/62.6 | 12.7/0.06/62.4 | 13.1/0.06/62.4 | 69.2/0.03/57.6 | 69.3/0.03/57.6 |
| 64/256; 1; up | 13.9/0.06/62.7 | 20.8/0.03/61.8 | 20.2/1.04/61.8 | 12.3/1.03/61.9 | 14.4/0.06/61.8 | 69.3/0.03/58.3 | 69.3/0.03/58.0 |
| 64/256; 8; down | 26.2/1.03/62.1 | 28.2/1.03/64.3 | 25.7/1.03/63.0 | 26.7/1.03/62.9 | 32.2/0.06/63.2 | 123.9/0.03/60.6 | 134.8/0.03/60.0 |
| 64/256; 8; up | 27.5/1.03/61.3 | 29.1/1.03/61.2 | 25.1/1.03/62.4 | 26.1/1.03/62.9 | 35.1/0.23/61.1 | 128.7/0.03/60.4 | 121.3/0.23/60.5 |
| 64/1024; 1; down | 14.0/0.04/63.2 | 10.8/1.03/61.5 | 15.8/1.03/63.3 | 12.5/1.03/62.1 | 13.4/0.04/63.2 | 84.0/0.03/58.8 | 121.0/0.03/59.0 |
| 64/1024; 1; up | 12.4/1.03/62.4 | 16.2/0.04/62.2 | 16.8/1.03/63.9 | 11.5/1.03/62.3 | 13.7/0.06/61.9 | 80.3/0.03/58.2 | 121.1/0.03/59.8 |
| 64/1024; 8; down | 27.4/1.03/62.5 | 25.2/1.03/61.7 | 25.6/1.03/63.7 | 25.2/1.03/62.4 | 37.6/0.23/61.8 | 128.4/0.03/59.5 | 134.5/0.23/60.2 |
| 64/1024; 8; up | 26.4/1.03/61.8 | 26.7/1.03/62.1 | 26.0/1.03/62.4 | 27.6/1.03/61.7 | 34.8/0.06/62.3 | 128.3/0.03/62.8 | 135.8/0.03/61.8 |
| 128/256; 1; down | 65.1/0.03/59.3 | 20.5/0.03/62.9 | 65.4/0.03/61.5 | 65.6/0.03/61.3 | 65.2/0.03/61.4 | 69.5/0.03/58.0 | 69.4/0.03/57.4 |
| 128/256; 1; up | 63.7/0.03/60.6 | 20.9/0.03/61.3 | 63.2/0.03/61.6 | 64.7/0.03/59.6 | 63.4/0.03/61.9 | 69.3/0.03/58.6 | 69.3/0.03/58.4 |
| 128/256; 8; down | 106.9/1.03/65.1 | 127.0/1.03/65.2 | 94.2/1.03/65.5 | 116.6/1.03/66.0 | 136.7/0.04/65.9 | 133.1/0.04/66.7 | 141.6/0.04/67.1 |
| 128/256; 8; up | 108.3/1.03/66.3 | 137.9/1.03/68.1 | 110.6/2.03/67.5 | 110.6/1.03/66.1 | 135.0/0.04/67.5 | 136.6/0.04/67.4 | 135.1/0.23/67.8 |
| 128/1024; 1; down | 75.3/1.03/61.7 | 84.7/0.03/60.9 | 111.1/1.03/65.7 | 12.6/1.03/62.5 | 90.8/0.03/61.0 | 94.8/0.03/58.3 | 132.3/0.03/65.1 |
| 128/1024; 1; up | 89.9/1.03/60.4 | 82.3/0.03/61.4 | 24.0/1.03/63.1 | 12.4/1.03/62.9 | 87.5/0.03/60.0 | 92.2/0.03/59.3 | 132.0/0.03/65.0 |
| 128/1024; 8; down | 85.0/1.03/65.1 | 110.9/1.03/65.6 | 96.6/1.03/64.5 | 112.3/2.03/65.7 | 133.0/0.05/66.1 | 135.2/0.04/65.4 | 140.3/0.04/66.7 |
| 128/1024; 8; up | 108.8/1.03/64.8 | 116.1/1.03/63.6 | 91.7/1.03/65.7 | 106.1/1.03/66.8 | 133.2/0.04/66.8 | 137.4/0.04/67.5 | 141.6/0.23/67.4 |
| 256/256; 1; down | 65.7/0.03/60.0 | 20.7/0.03/63.5 | 65.2/0.03/60.3 | 66.2/0.03/61.2 | 65.4/0.03/59.5 | 69.3/0.03/58.2 | 69.4/0.03/57.9 |
| 256/256; 1; up | 63.1/0.03/60.6 | 20.6/0.03/62.7 | 63.6/0.03/61.0 | 64.6/0.03/60.1 | 63.3/0.03/62.8 | 69.3/0.03/58.5 | 69.4/0.03/57.5 |
| 256/256; 8; down | 115.7/1.04/79.5 | 142.4/0.03/71.0 | 102.0/1.03/79.0 | 114.5/1.03/78.3 | 141.4/0.04/79.3 | 141.2/0.04/79.5 | 141.8/0.04/80.8 |
| 256/256; 8; up | 117.0/1.04/78.3 | 143.7/0.03/72.5 | 102.9/1.04/78.9 | 113.1/1.04/80.2 | 141.9/0.04/79.5 | 142.5/0.04/80.0 | 142.8/0.04/81.9 |
| 256/1024; 1; down | 113.6/1.03/65.9 | 84.0/0.03/59.6 | 90.8/1.03/72.7 | 14.7/1.03/62.6 | 105.1/0.04/59.8 | 109.0/0.04/58.0 | 139.4/0.04/77.4 |
| 256/1024; 1; up | 14.1/1.03/63.4 | 84.1/0.03/59.4 | 87.6/1.03/80.9 | 99.9/1.03/64.0 | 101.9/0.04/61.4 | 109.9/0.04/60.7 | 139.4/0.04/79.8 |
| 256/1024; 8; down | 116.1/1.03/77.2 | 114.1/1.03/79.8 | 111.1/1.03/76.2 | 117.4/1.03/79.1 | 139.7/0.04/80.3 | 140.9/0.04/79.8 | 141.4/0.04/80.3 |
| 256/1024; 8; up | 121.1/1.04/80.9 | 115.1/1.04/80.2 | 109.1/1.04/79.7 | 118.8/1.03/79.4 | 141.8/0.04/80.6 | 143.6/0.04/80.2 | 144.1/0.04/80.5 |

### 250 Mbit/s, RTT 25 ms

| Q/W KiB; flows; dir | Stock | Burst8 | Cubic | KA1s | Recovery R | Paced R | Final C |
|---|---|---|---|---|---|---|---|
| 64/256; 1; down | 12.8/1.03/62.5 | 11.0/1.03/63.0 | 10.7/1.03/61.7 | 12.4/1.03/62.8 | 13.3/0.06/61.8 | 69.4/0.03/57.2 | 69.5/0.03/57.8 |
| 64/256; 1; up | 8.8/0.06/62.4 | 11.6/1.03/62.2 | 16.8/1.03/62.0 | 11.2/1.03/62.6 | 12.2/0.06/62.0 | 68.9/0.03/57.9 | 68.9/0.03/57.7 |
| 64/256; 8; down | 24.8/1.03/62.3 | 26.1/1.03/62.7 | 24.1/1.04/62.8 | 24.7/1.03/62.4 | 22.9/0.06/63.0 | 151.1/0.03/58.5 | 155.2/0.03/58.0 |
| 64/256; 8; up | 26.5/1.03/61.8 | 27.4/1.03/62.2 | 27.5/1.03/62.2 | 29.0/1.03/61.5 | 25.7/0.06/61.2 | 148.6/0.03/58.5 | 155.0/0.03/58.9 |
| 64/1024; 1; down | 11.2/0.04/61.3 | 6.8/1.03/61.5 | 11.7/1.03/62.7 | 8.1/1.03/62.9 | 9.9/0.05/61.8 | 77.5/0.03/57.8 | 148.6/0.03/58.0 |
| 64/1024; 1; up | 8.8/0.06/62.9 | 11.5/1.03/61.9 | 14.9/1.03/62.5 | 11.0/1.03/61.9 | 12.1/0.06/61.8 | 78.7/0.03/57.6 | 151.3/0.03/57.7 |
| 64/1024; 8; down | 28.8/1.03/61.4 | 27.4/1.03/62.2 | 28.9/1.03/62.1 | 24.6/1.03/62.2 | 28.5/0.06/62.7 | 151.8/0.03/58.5 | 154.2/0.03/58.1 |
| 64/1024; 8; up | 28.5/1.03/61.7 | 25.3/1.03/61.9 | 27.9/1.03/62.6 | 26.1/1.03/62.3 | 28.4/0.23/62.0 | 152.2/0.03/59.0 | 153.3/0.23/59.0 |
| 128/256; 1; down | 22.0/1.03/63.4 | 20.3/0.03/62.4 | 24.8/1.03/62.6 | 22.0/1.03/62.6 | 25.8/0.03/62.0 | 69.1/0.03/57.5 | 69.3/0.03/57.1 |
| 128/256; 1; up | 26.6/0.03/62.3 | 20.4/0.03/61.6 | 23.8/1.03/62.7 | 19.3/1.03/62.4 | 24.4/0.03/62.1 | 68.7/0.03/59.8 | 68.8/0.03/58.5 |
| 128/256; 8; down | 51.9/1.03/63.1 | 59.2/1.03/62.9 | 51.0/1.03/62.4 | 51.1/1.03/62.7 | 50.5/0.06/61.5 | 204.5/0.03/60.9 | 223.3/0.23/60.8 |
| 128/256; 8; up | 61.5/1.03/62.7 | 54.3/1.03/63.4 | 55.1/1.03/61.6 | 49.9/1.03/61.9 | 74.8/0.06/61.9 | 207.8/0.03/60.2 | 228.0/0.03/61.7 |
| 128/1024; 1; down | 11.8/1.03/62.0 | 18.8/0.04/62.1 | 20.3/1.03/62.3 | 22.9/1.03/62.0 | 13.6/0.06/62.4 | 132.0/0.03/57.9 | 205.2/0.03/60.9 |
| 128/1024; 1; up | 22.5/1.03/62.2 | 19.3/1.03/62.0 | 28.3/1.04/61.7 | 21.5/1.03/61.3 | 25.6/0.05/61.3 | 131.8/0.03/57.6 | 217.2/0.03/60.3 |
| 128/1024; 8; down | 55.3/1.03/62.7 | 47.5/1.03/62.9 | 90.1/1.03/62.6 | 58.3/1.03/64.0 | 51.8/0.06/60.9 | 216.8/0.03/60.8 | 226.3/0.23/60.8 |
| 128/1024; 8; up | 56.2/1.03/62.1 | 42.0/1.03/62.1 | 50.1/1.03/62.4 | 53.3/1.03/61.8 | 195.9/0.04/60.6 | 213.5/0.03/61.5 | 226.4/0.03/62.7 |
| 256/256; 1; down | 58.8/0.03/64.2 | 20.4/0.03/62.5 | 60.9/0.03/62.5 | 59.7/0.03/62.5 | 59.6/0.03/63.3 | 69.3/0.03/56.4 | 69.2/0.03/57.7 |
| 256/256; 1; up | 58.6/0.03/61.7 | 20.4/0.03/62.1 | 61.3/0.03/60.8 | 58.6/0.03/61.3 | 58.1/0.03/61.2 | 68.9/0.03/58.3 | 68.8/0.03/57.6 |
| 256/256; 8; down | 172.6/1.03/68.3 | 175.4/0.03/59.4 | 186.0/1.03/66.4 | 152.4/1.03/67.9 | 229.6/0.04/69.1 | 232.1/0.04/68.0 | 234.6/0.04/69.2 |
| 256/256; 8; up | 152.5/1.03/68.2 | 175.4/0.03/60.9 | 170.8/1.03/69.3 | 193.1/1.03/69.1 | 228.1/0.04/69.3 | 224.8/0.04/69.5 | 234.7/0.04/70.1 |
| 256/1024; 1; down | 208.5/0.03/68.3 | 78.9/0.03/66.3 | 143.9/1.03/67.2 | 112.9/1.03/68.3 | 209.4/0.03/68.5 | 186.3/0.03/67.9 | 230.2/0.03/68.7 |
| 256/1024; 1; up | 201.8/0.03/68.8 | 76.0/0.03/62.3 | 202.2/0.03/69.8 | 205.3/0.03/69.7 | 205.9/0.03/69.9 | 230.9/0.03/69.1 | 230.9/0.03/69.4 |
| 256/1024; 8; down | 151.9/1.03/66.6 | 149.9/1.03/67.4 | 178.7/1.03/68.6 | 157.1/1.03/68.3 | 222.9/0.04/68.2 | 229.2/0.04/68.2 | 235.4/0.04/68.1 |
| 256/1024; 8; up | 88.1/2.03/61.9 | 133.1/1.03/67.2 | 137.0/1.03/68.8 | 138.4/1.03/66.8 | 224.7/0.04/70.1 | 233.4/0.04/69.1 | 235.5/0.04/69.6 |

### No-loss controls, uncapped bandwidth

| RTT ms; W KiB; flows; dir | Stock | Burst8 | Cubic | KA1s | Recovery R | Paced R | Final C |
|---|---|---|---|---|---|---|---|
| 0; 256; 1; down | 7798.4/0.01/0.4 | 7202.2/0.00/0.4 | 7087.7/0.00/0.4 | 7173.5/0.00/0.3 | 7833.8/0.00/0.4 | 7870.8/0.00/0.4 | 7786.4/0.00/0.4 |
| 0; 256; 1; up | 9621.8/0.00/0.5 | 7913.1/0.00/0.4 | 8583.2/0.00/0.5 | 8540.2/0.00/0.6 | 9112.8/0.00/0.5 | 9033.9/0.01/0.5 | 9323.7/0.00/0.5 |
| 0; 256; 8; down | 19975.8/0.00/2.0 | 17343.3/0.00/1.3 | 16675.9/0.00/2.5 | 19308.8/0.00/2.1 | 17767.4/0.00/2.4 | 17684.0/0.00/2.4 | 18805.3/0.00/2.2 |
| 0; 256; 8; up | 13851.6/0.00/2.3 | 13915.5/0.00/1.6 | 13852.3/0.00/2.4 | 13879.2/0.01/2.1 | 14131.2/0.00/2.3 | 14614.3/0.00/2.1 | 14327.7/0.00/2.2 |
| 0; 4096; 1; down | 7921.2/0.00/0.8 | 7052.0/0.00/0.8 | 7134.4/0.01/0.9 | 7161.3/0.00/0.8 | 7825.1/0.00/0.8 | 7828.8/0.00/0.8 | 7781.2/0.00/0.8 |
| 0; 4096; 1; up | 9277.2/0.01/1.3 | 8278.4/0.00/1.9 | 8693.0/0.01/1.9 | 8442.0/0.01/2.9 | 9411.8/0.01/2.8 | 9187.6/0.01/2.7 | 9367.3/0.01/2.5 |
| 0; 4096; 8; down | 17299.6/0.01/31.8 | 16575.9/0.01/11.7 | 15183.8/0.01/38.2 | 16218.8/0.02/34.7 | 17581.1/0.01/27.2 | 14930.1/0.02/39.8 | 14584.7/0.01/40.6 |
| 0; 4096; 8; up | 14126.9/0.00/2.0 | 14371.7/0.00/2.0 | 14215.3/0.00/2.4 | 13317.5/0.00/2.4 | 14277.2/0.00/2.2 | 14468.7/0.01/2.0 | 14609.5/0.01/2.2 |
| 40; 4096; 1; down | 558.2/0.05/93.1 | 161.7/0.05/94.0 | 459.9/0.05/93.1 | 367.9/0.05/92.7 | 450.3/0.05/93.5 | 677.5/0.04/88.3 | 675.1/0.05/89.0 |
| 40; 4096; 1; up | 327.9/0.05/92.6 | 169.7/0.05/92.7 | 305.6/0.05/92.7 | 303.5/0.05/93.1 | 302.3/0.05/93.4 | 665.4/0.05/89.3 | 665.1/0.04/88.9 |
| 40; 4096; 8; down | 5268.6/0.05/94.2 | 1667.2/0.05/95.7 | 4918.3/0.05/93.7 | 5393.2/0.05/92.6 | 5527.2/0.05/93.2 | 5735.2/0.04/90.0 | 5826.4/0.05/88.6 |
| 40; 4096; 8; up | 3916.4/0.05/106.7 | 1343.4/0.05/103.7 | 4605.5/0.05/100.5 | 4744.6/0.05/96.1 | 3699.8/0.05/105.3 | 5701.8/0.04/90.0 | 5664.4/0.04/90.9 |

## Mitigations and policy choice

- **Burst size:** the effective capabilities are cached from
  `NoopCapDev::with_caps` in `ts_netstack_smoltcp_core/src/lib.rs`, not from
  `WakingPipeDev` at each poll. Stock sets only MTU there. In 0.14 this knob
  clamps the advertised *scaled window field*, not outgoing bursts: 8 gives
  roughly 79 KiB effective window at a 256 KiB buffer and 310 KiB at 1 MiB.
  It trades rate for fewer drops and still stalls; median goodput falls to 27.8.
- **Cubic alone:** stock-library median 58.0 versus Reno 57.4; maximum pause is
  still 2.03 s. Stock Cubic needs smoltcp's `std` feature because its original
  arithmetic fails `no_std`; the private source fixes that without replacing
  the algorithm.
- **Keepalive:** the deterministic B2 reproduction recovers at **180.12 s** with
  60 s keepalive and **3.12 s** with 1 s keepalive for a responsive peer.
  Silent peers still close at 120.02 s. The lossy matrix still pauses for 2.03 s:
  keepalive is a backstop, not multi-hole recovery.
- **Window:** 256 KiB limits a healthy single flow to ~69 Mbit/s in this harness.
  1 MiB is useful after recovery/pacing works: ~15–17 MB/s at 150 and ~19–29 MB/s
  at 250 for a single download. It allocates ~2 MiB/socket eagerly (~2 GiB per
  1,000 sockets); retain 256 KiB on dense, memory-constrained nodes.
- **Pacing:** corrected unpaced recovery removes long stalls but median rate is
  only 61.5. Pacing raises Reno to 130.2; Cubic reaches 140.9 and improves the
  large-window single flow. Reno's maximum gap is smaller (0.04 s versus
  Cubic's 0.23 s), but Cubic is the throughput choice here. Pacing bypasses
  low-baseline RTT and unknown estimates; ACKs and first fast repair remain
  immediate. A fuller queue can increase probe p95 while improving goodput.

No-loss data are mixed at CPU saturation: final 0-RTT/4-MiB/eight-download
rate is 14.58 Gbit/s versus stock 17.30, while the 40-ms single-flow rows
roughly double. This is a material lab trade, not a claim of no regression.
Two additional interleaved 30 s pairs for that row give median **17.78 → 16.12 Gbit/s** (-9.4%). [Repeat records](../scratch/tcp-0.14/docs/tcp-workaround-30-noloss-repeats.jsonl).

## Safety and carrying cost

**400,000 randomized socket rounds pass**, seeds 11/22/33/44 × 100,000,
400 random steps and the original 20,000-step cooperative drain bound.
Total: 2,110,383,432 emitted segments, 396,585,417 ACKs;
**0 panics, 0 wedged sockets**, maximum consecutive immediate polls without
emission **2**. The 2,000-round preflight also passed.
[Seed summaries](../scratch/tcp-0.14/docs/tcp-workaround-30-fuzz.json). Long runs disable per-packet test
logging; PRNG draws and socket behavior remain the review-26 harness.

Passed: **724 smoltcp library tests**; forwarder and all three netstack crates'
unit/integration/doctests with all features; workspace and standalone vendor
clippy `-D warnings`; workspace/vendor fmt; `no_std` without alloc for Reno,
Reno+Cubic+IPv4/IPv6+defmt, and no congestion controller. Native target:
`aarch64-apple-darwin`, Rust 1.95.0; no cross-target/16-bit claim.

Use **vendored source**, not an external scratch path or floating fork: the
import, checksum, exact version, path override and lockfile make the delta
reviewable and removable. B1/B2, extra closed-window persist, upstream #1211
API, NewReno, reviewed SACK/Pipe, optional pacer and netstack policy are distinct
commits. The scoreboard contains the `4e6b554` flight validation and walk bounds
from its first commit. SACK recovery uses the fixed loss window with Pipe,
consistent with [RFC 6675 §5](https://datatracker.ietf.org/doc/html/rfc6675#section-5);
rescue/RACK are absent. Existing Reno/Cubic algorithms remain upstream apart
from recovery hooks and the tiny Cubic portability fix. No production dependency
was added. The 32 ranges use 256 bytes of endpoints plus bookkeeping/socket state.

A future 0.15 port should first drop #1211 and any B1/B2/portability fixes that
are included, then adapt the remaining socket/recovery hooks and rerun this
matrix/harness. Estimate **half a day to two days** for a routine rebase plus
validation; an upstream sender-SACK redesign requires a fresh comparison, not
blind patch application. Drop the vendor override once upstream supplies the
required behavior. This is a smaller carry than the old 0.13.1 CC replacement.

Builds/tests use at most four jobs/workers; timed benches run sequentially with
no concurrent heavy work. `nice -n 10` is invoked throughout, but the sandbox
rejects `setpriority`; scheduling priority could not actually be lowered.
No real-client Wi-Fi/TLS, live-node rollout or daemon change was performed.

## Implementation commits

- `9af3f91` — bench(forwarder): restore bottleneck recovery and loaded-latency measurements
- `038876f` — build: pin published smoltcp 0.14.0 source for private recovery patches
- `050cb5c` — smoltcp: retain fast repair after failed device emission (B1)
- `22e892c` — smoltcp: restore outstanding-flight RTO on zero-window reopen (B2)
- `e04f3fa` — smoltcp: backport per-socket RTO bounds from upstream #1211
- `b8087e5` — smoltcp: retain NewReno recovery through partial ACKs
- `94847a3` — smoltcp: add opt-in bounded sender SACK with review-26 validation
- `63e16c9` — smoltcp: use core-compatible Cubic arithmetic for no_std builds
- `6a9daf5` — test(smoltcp): retain reviewer socket harness and controller regressions
- `5952e3a` — smoltcp: preserve persist after RTO on a closed window
- `46be14c` — test(smoltcp): cover SACK negotiation and reneging on RTO
- `21e8417` — smoltcp: add optional bounded pacing for remaining shallow-queue loss
- `9da013c` — netstack: select Cubic recovery, bounded pacing and a 200 ms RTO floor
- `58f75d0` — style(smoltcp): satisfy Rust 1.95 formatting and strict clippy

Report/artifacts commit: `cb547f7` — docs: record Brief 30 matrix and safety results.

Reproduction: [matrix runner](../scratch/tcp-0.14/docs/tcp-workaround-30-matrix.py),
[temporary stock mitigation hooks](../scratch/tcp-0.14/docs/tcp-workaround-30-mitigation-hooks.patch),
[measured binary SHA-256s](../scratch/tcp-0.14/docs/tcp-workaround-30-binaries.json),
[vendor patch notes](../scratch/tcp-0.14/vendor/smoltcp/PATCHES.md). The stock hook patch applies
at the bench-only commit `9af3f91`; env knobs are `BENCH_BURST=8`,
`BENCH_CC=cubic`, or `BENCH_KEEPALIVE_MS=1000`. Production contains no bench env
hooks. Benchmark compilation and the long randomized-test command are in the
vendor patch notes. Example: `RTT_MS=25 BW_MBPS=150 QUEUE_KB=64 TCP_BUF_KB=1024 LATENCY=1
nice -n 10 <bench-binary> 15 1 down`. The matrix runner covers all rows.

## 30b — review-31 pacing debt fix

2026-10-15. LOW-1: each successful send now clamps `pacer.next` to `now + SRTT`.
Immediate fast repairs still bypass the budget, but repeated partial ACKs cannot
accumulate debt beyond one RTT. The regression fails before the fix and passes
after it, including an RTT decrease with existing debt. Only `pacing.rs` changes
in production; the netstack policy and scoreboard remain unchanged.

The unchanged extended review-31 harness ran against a scratch copy with the
reviewer's two field-visibility changes. Each row below used 100k rounds × 400
hostile steps, then the `poll_at`-driven drain; no `log` feature.

| Build | Seeds | Maximum scheduled pacing debt |
|---|---|---|
| Release | 11 / 22 / 33 / 44 | 6663 / 8055 / 5225 / 7676 ms |
| Release, overflow checks + debug assertions | 101 | 6408 ms |

Across the required **500k rounds**: 6.802 billion polls, 375,011 wrap rounds,
64.7 million failed emits (801,180 failed repairs), and 13.9 million partial ACKs
in recovery. **Zero panics, wedges, drain stalls, unarmed sockets, empty storms
or data sent after `Ingress`; worst consecutive `Now` without emission: 1.**
Maximum debt: **8.055 s**; a separate 100k-round seed-22 replay with read-only
logging confirms **SRTT = 9.667 s** at that maximum. The bound prevents accumulated
repair debt; it deliberately preserves peer-inflated RTTs. The targeted repair
case reports 178 ms without SACK and 181 ms with SACK.

Validation passes: smoltcp **725 tests** (one long harness ignored), netstack and
forwarder **183 tests + five doctests**, workspace/vendor clippy `-D warnings`,
both fmt checks, and no_std Reno/Cubic/no-CC builds. LOW-2 is left unchanged:
Pipe caching would require broader recovery-state invalidation changes; its
existing bounded cost remains acceptable for this narrow fix.

[30b validation record](../scratch/tcp-0.14/docs/tcp-workaround-30b-validation.json) records run counters,
source/binary hashes and quality commands. Raw commands/logs: `scratch/tcp30b-logs/`;
runner: `scratch/tcp30b-fuzz.py`. Build with the recorded no-default feature set;
run the ignored `socket::tcp::test::review31_random_peer_inputs` test with
`R_ROUNDS=100000 R_STEPS=400 R_SEED=<seed> R_VERBOSE=0 --exact --ignored --nocapture
--test-threads=1`. Checked release additionally uses
`RUSTFLAGS='-C overflow-checks=yes -C debug-assertions=yes'`.
Separate commit: `92c7b2e` — `smoltcp: bound accumulated pacing debt to one smoothed RTT`
(with the requested trailers). No live-node, main or remote changes.
