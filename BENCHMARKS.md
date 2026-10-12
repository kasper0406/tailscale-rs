# BENCHMARKS

Host: Mac mini M4 (10 cores, 16 GiB), macOS 26.6.2 (25G83), wired Ethernet. Fork `GeiserX/tailscale-rs`
@ `719aa889` + series on branch `ts-exit/runner` (commit given per table). Release builds,
`aarch64-apple-darwin`, rustc 1.95.0. Raw outputs: `bench/results/`.

Three levels, kept apart on purpose (PLAN: never present a tunnel-only number as exit-node throughput):

1. **Component (local, no tailnet)**: the exit-node forwarder alone, and the macOS UDP send path alone.
2. **Tunnel-only (live)**: test laptop → Rust node's own tailnet address. *Pending tailnet access.*
3. **End-to-end exit-node (live)**: laptop/iPad → Rust exit node → public internet. *Pending tailnet access.*

---

## 1a. Forwarder throughput: `ts_forwarder/benches/forward_throughput.rs`

A peer netstack dials a `0.0.0.0/0` destination over an in-memory pipe (overlay MTU 1280). The forwarder
netstack accepts it, dials a loopback server through `HostExitDialer`'s socket path and splices. No WireGuard,
no UDP underlay. `RTT_MS` delays the overlay pipe symmetrically, standing in for the peer's path latency.

```sh
cd repos/geiserx-tailscale-rs
FWD_MODE={ports|raw|gated} RTT_MS=<ms> TCP_BUF_KB=<KiB> \
  cargo bench -p geiserx_ts_forwarder --bench forward_throughput -- <secs> <flows> <up|down>
```

### Port mode: cold-port latency and per-packet cost (commit `ba175e7`)

One flow, down, 5 s. `cold connect` is the first connect to a port without a listener yet.

| mode | what | cold connect (RTT 20 ms) | Mbps (RTT 0) |
|---|---|---|---|
| ports | explicit listener (reference) | 26 ms | 10,431 |
| raw | all-port, raw-socket observer (fork baseline) | **3,032 ms** | 7,687 |
| gated | all-port, `PortGate` (this series) | **27 ms** | 10,516 |

Raw-observer mode loses the first SYN to every new destination port, so the client waits out a SYN
retransmit. smoltcp's initial RTO is 3 s; iOS/macOS use 1 s. It also copies every inbound packet to learn
ports, which costs about 27% throughput. The gate removes both.

### Single-flow window limit (commit `5e872d8`, explicit-port mode)

| RTT | TCP buffer/direction | down Mbps | up Mbps | 
|---|---|---|---|
| 0 ms | 256 KiB (default) | 7,830 | 8,008 |
| 10 ms | 256 KiB | 115 | 111 |
| 40 ms | 256 KiB | 36.6 | 35.3 |
| 40 ms | 1 MiB | 177 | |
| 40 ms | 4 MiB | 728 | |

The netstack has no window autotuning, so one flow is capped near `buffer / RTT`. At the default 256 KiB, an
iPad on cellular (30–60 ms RTT to home) gets about 30–60 Mbit/s per TCP flow through the exit node, whatever
the ISP speed. Buffers are `vec![0; n]` (calloc), so the cost of a larger setting is mostly virtual memory until
data is actually queued (measured below).
The runner exposes this as `--tcp-buffer-size`.

**Memory cost** (`bench/results/forwarder-rss-buf.txt`; gated mode, RTT 40 ms, down, 6 s, every flow saturating;
peak RSS of the whole bench process, i.e. peer *and* forwarder netstack, both with the given buffer):

| buffer | 1 flow: Mbps / peak RSS | 32 flows: Mbps / peak RSS |
|---|---|---|
| 256 KiB | 42 / 6 MiB | 1,431 / 42 MiB |
| 1 MiB | 164 / 8 MiB | 5,670 / 141 MiB |
| 4 MiB | 724 / 19 MiB | 12,200 / 554 MiB |

RSS tracks bytes actually in flight, not the configured size (calloc'd buffers stay virtual until touched).
Worst case is about 2 × buffer per *saturating* flow on the forwarder, so 4 MiB × 100 bulk flows would be
~800 MiB. Browsing flows rarely fill their windows. For the live A/B: 256 KiB (fork default) vs 2 MiB vs 4 MiB.

*Caveat:* the in-memory pipe has no loss and no reordering. smoltcp's loss recovery (no SACK) will do worse
on a real path, which matters more as the window grows.

## 1b. macOS UDP send path: `bench/udp-send`

How much CPU each way of sending one underlay datagram costs. Loopback to an in-process discard sink, so
`cpu_us_per_pkt` includes the receive side and NIC/driver effects are excluded. 1240-byte payload, 2 s per row,
3 runs (`bench/results/udp-send-loopback-*.txt`).

```sh
cd bench/udp-send && cargo build --release && ./target/release/udp-send sink 2 1240 32
```

| send path | packets/s (3 runs) | CPU µs/packet |
|---|---|---|
| unconnected `sendto` (**fork magicsock today**) | 200k / 199k / 205k | 7.9 / 7.9 / 7.7 |
| unconnected `sendto` + `IP_DONTFRAG` | 219k / 241k / 239k | 7.3 / 6.9 / 6.9 |
| connected `send` | 361k / 381k / 368k | 4.9 / 4.7 / 4.8 |
| connected `send` + `IP_DONTFRAG` | 573k / 575k / 542k | 3.4 / 3.3 / 3.3 |
| connected `sendmsg_x` ×32 + `IP_DONTFRAG` | 1,047k / 1,001k / 1,068k | 1.9 / 2.0 / 1.8 |

Median CPU per packet goes from 7.9 µs (unconnected) to 4.8 (connected), 3.3 (+DF) and 1.9 (+`sendmsg_x`),
about 4x less. Earlier exploratory runs gave the same ordering.

This matches the mechanism behind tailscale/tailscale#21602 (connected sockets + DF on darwin; DF lets macOS
skip the random IPv4 ID, and connecting saves the per-packet route/PCB lookup). It points Phase 3A (connected
per-peer underlay sockets) and 3B (`sendmsg_x`/`recvmsg_x`) at the fork's `ts_magicsock`, which today sends
every packet with `send_to` on one shared unconnected socket. Loopback is a lower bound on the effect: #21602
reports Macs in everyday use settling near 11k packets/s on the unconnected path.

---

## 1c. Tunnel-only, live on this Mac (`runner/tunnel-bench.sh`)

The official Go client (built from `repos/tailscale-go` @ 3bb8a906, userspace `tailscaled` in
`state/go-client`) moves 1 GB over WireGuard to the Rust node's own tailnet address (`--sink-port`
service), each direction. The path is direct over the LAN (192.168.50.76:41642), with no internet and no
exit forwarding. Both nodes share the Mac, so throughput is partly bounded by the Go client. The comparable
metric is **the Rust node's CPU seconds per GB**. `exitnode` build profile (opt-level 3).
Raw: `bench/results/tunnel-ab.csv`. Profiles: `scratch/prof/sample*.txt`.

| variant (cumulative) | down Mbit/s | down CPU-s/GB | up Mbit/s | up CPU-s/GB |
|---|---|---|---|---|
| baseline (`runner` @ dceb1a3) | 1,178 | 18.6 | 1,860 | 11.6 |
| + packet-path spans at trace (`752cf20`) | 1,282 | 15.7 | 1,870 | 10.5 |
| + egress batching, zero-copy (`4124a9e`) | ~1,310 | 11.9 | ~1,850 | 10.5 |
| + 7 MiB UDP socket buffers (`6f93451`) | ~1,280 | ~12.0 | ~1,850 | 10.5 |
| + connected sockets + DF (`perf-connected`, opt-in) | **1,760** | **8.9** | **1,950** | **9.9** |
| connected + DF alone on the baseline | 1,433 | 14.6 | 1,875 | 11.9 |

Medians of 3–5 runs. In every series up to here, run 2 (the second download) was an outlier (362–995 Mbit/s):
the node's path choice briefly switched from the peer's LAN endpoint to its public one, a NAT hairpin through
the router. Fixed by porting Go's full `betterAddr` scoring, which prefers private paths (`ts-exit/runner`).
After that fix, 5 of 5 runs of the combined build landed at 1,749–1,760 Mbit/s down and 8.9 CPU-s/GB.

Profile after these changes (download): UDP syscalls 26% of busy CPU (one `sendto` per packet), crypto 21%
(RustCrypto ChaCha20-Poly1305 with the portable Poly1305 backend), malloc/memcpy 12%, tokio/locks/clock 15%,
tracing down from 16% to 2.5%. Next: `ring` AEAD (microbench 2.6x faster than RustCrypto here, in
progress), `sendmsg_x`/`recvmsg_x` batching, and fewer per-packet allocations.

Later steps (5 × 1 GB, connected + DF on; `bench/results/tunnel-ab.csv`):

| variant | down Mbit/s | down CPU-s/GB | up CPU-s/GB | profile change |
|---|---|---|---|---|
| combined (above, + `betterAddr`) | 1,757 | 8.9 | 9.9 | |
| + `ring` AEAD (`--features ring-aead`) | 1,765 | 9.2 | 10.3 | crypto 21% → 12% of busy |
| + `sendmsg_x` batched sends (opt-in) | 1,800 | 9.1 | 10.2 | send syscalls 23% → 14% |

Both cut their own cost as expected, but **CPU per GB doesn't move**. The node is not saturated in this
setup: the co-located Go client needs 5.2 cores to receive 1.75 Gbit/s while the node uses 2.1. An
unsaturated node idles between packets, so work saved per packet becomes smaller batches and more wakeups.
What remains is per-packet handoff overhead: each packet crosses 4–5 actor/channel hops
(netstack → dataplane → underlay → magicsock), and tokio, locks, clock and allocation make up ~40% of busy
time. Go's reference work reports the same wall (~30% scheduler) and the same fix: fewer handoffs. At this
home line's ~750 Mbit/s the node has 2–3× headroom either way.

**Tokio worker count** (same binary; `TOKIO_WORKER_THREADS`, now the runner's `--worker-threads`).
Latency is a 64-byte echo through the tunnel (`runner/rttprobe.py`) while the load runs.

Single flow:

| workers | down Mbit/s | down CPU-s/GB | up CPU-s/GB | echo p99 under load |
|---|---|---|---|---|
| 10 (default, one per core) | ~1,800 | 9.1 | 10.2 | 1.8 ms |
| 2 | ~1,940 | 6.4 | 7.0 | 1.7 ms |
| 1 | ~2,063 | 3.6 | 4.4 | **1,004 ms** (max 3,016) |

Eight parallel flows (500 MB each) plus the probe:

| workers | aggregate Mbit/s | CPU-s/GB | echo p99 / max |
|---|---|---|---|
| 10 | 760 | 15.2 | 12.0 / 12.2 ms |
| **4 (runner default)** | **1,127** | **10.6** | **12.1 / 12.3 ms** |
| 2 | 904 | 13.0 | **3,016 / 4,002 ms** |

More workers than needed cost CPU in cross-thread wakeups (tokio unparks idle workers to steal work, and
each packet passes several tasks). Too few let bulk per-flow tasks starve the serial packet pipeline, whose
bounded ingress queue (4,096 packets) then drops packets: the 1 s/3 s stalls are TCP retransmit timeouts.
2 looked best on one flow but stalls with concurrency, so the runner defaults to 4. The 1-worker number
(3.6 CPU-s/GB) bounds what a dedicated, unstarved packet-pipeline thread could save. That restructure is
on branch `ts-exit/pipeline-thread` (in progress).

**Dedicated packet-pipeline thread** (branch `ts-exit/pipeline-thread`, opt-in
`TS_EXPERIMENT_PIPELINE_THREAD=1`). Controlled A/B with the **same binary**, knob 0/1/1/0, 4 caller workers,
256 KiB buffers (`bench/results/pipeline-controlled.txt`):

| | off | on |
|---|---|---|
| 1 download: Mbit/s, CPU-s/GB | ~1,810, 8.8–8.9 | ~2,015, **4.6–4.8 (−47%)** |
| 1 upload: Mbit/s, CPU-s/GB | ~1,930, 9.8–10.0 | ~1,790, **6.5–6.7 (−33%)** |
| 8 flows: aggregate Mbit/s (typical), CPU-s/GB | ~1,715, 9.9–14.2 | **~1,235**, 5.4–7.8 |
| 8 flows: runs with 1–4 s echo stalls | 1 / 4 | **2 / 4** |

During 8 flows the pipeline thread runs at 86% of a core while the four caller workers idle at ~5%: one
thread is a hard ceiling, and at it throughput drops and stalls appear. Codex's critique
(`docs/codex-08-pipeline-critique.md`) predicted this and judged the mechanism overstated: its prior was
0–30% savings, and the measured single-flow saving exceeds that. Kept **opt-in**.

**Through the exit node to the internet, end state** (`runner/perf.sh`, OVH 200 MB, connected + DF +
batching + `ring`, 2 workers; before the concurrency finding): 654 and 755 Mbit/s at **10.1 and 6.9 CPU-s/GB** (81% and 64% of one core).
The release build of the original fork used 24.7–25.2 CPU-s/GB at the same rate (~2.2 cores).

Through the exit node to the internet (same Go client, OVH test file, `runner/perf.sh`) the node
reached ~740–760 Mbit/s, the home line's ceiling (direct from the Mac: 737–767 Mbit/s). The release build
used 24.7–25.2 CPU-s/GB there, the `exitnode` profile 22.7–23.2, and with connected sockets + DF 17.4.

## 2./3. Live tunnel and exit-node results

Not run yet: needs the auth key and route approval (RUNBOOK §0–4). Planned matrix, each ≥3 runs, median and
spread, with the path (direct/DERP) from the runner log quoted per run:

| scenario | client → target | metric |
|---|---|---|
| ceiling | laptop raw (no Tailscale) → public speed test / iperf3 server | Mbps up/down, RTT |
| ceiling | Mac raw → same | Mbps up/down |
| tunnel-only | laptop → `rust-exit` (`--no-advertise`), TCP echo/iperf-style sink on the node's netstack | Mbps, pps |
| exit e2e, 1 flow | laptop via `rust-exit` → public iperf3 server | Mbps up/down, retransmits |
| exit e2e, 4/8 flows | same, `-P 4`, `-P 8` | Mbps |
| exit e2e, UDP | `iperf3 -u -b ...` | loss, jitter |
| latency | `ping` / `curl -w` TTFB idle and under load | ms |
| A/B knobs | `--tcp-buffer-size` 256K vs 2M vs 4M | Mbps at the real RTT |
| stability | iPad streaming ≥ 60 min | stalls, reconnects, RSS/CPU over time |

## 4. Stage-0 scaling baseline: synthetic peers through the real node (2026-10-12)

Reference point before any sharding work (`docs/codex-10-sharding-design.md`). Binary `bin/exit_node-stage0b`
(branch `ts-exit/bench-scaling` @ `47b4714`, `ring-aead`, connected sockets + DF + `sendmsg_x` on, 4 tokio
workers, 256 KiB TCP buffers).

**Method.** Two enrolled tailnet nodes (`synthetic-load-1/2`, login-link enrolled, same user) are driven by
`examples/synthetic_peer`: its own WireGuard + smoltcp stack, no Go client, so the generator isn't the
~1.7 Gbit/s ceiling of §1c. Each peer holds one control session for the whole matrix that advertises its LAN
endpoint (`192.168.50.76:4170N`), because the node only learns private addresses from the netmap. Flows go to
`198.18.0.1:9000`, which the node redirects to a loopback sink (`TS_BENCH_EXIT_REDIRECT`, benchmark range →
loopback only, logged as a warning). So this measures the node's tunnel + forwarder + host-socket path, not the
internet. Generator, node and sink share the Mac mini (4 P + 6 E cores), so throughput is a lower bound and
**node CPU-s/GB is the comparable number**. Each cell: 3 runs, median. Transfers are 2 GB per flow (1-flow
loads), 250 MB (8 flows) and 30 MB (64 flows). Drivers: `runner/bench-node.sh`, `runner/stage0-matrix.sh`,
`runner/stage0-summary.py`. Raw data: `bench/results/stage0/{tokio4-fixed,pipeline1}/`.

| load | dir | default: Mbit/s (min–max) | node CPU-s/GB | node cores | generator cores | pipeline thread: Mbit/s | node CPU-s/GB |
|---|---|---|---|---|---|---|---|
| 1 peer × 1 flow | down | 3919 (3837–3928) | 3.02 | 1.5 | 0.9 | 1580 (1534–1613) | 7.26 |
| 1 peer × 1 flow | up | 2561 (2549–2584) | 6.60 | 2.1 | 1.2 | 2704 (2703–2713) | 4.52 |
| 2 peers × 1 flow | down | 6229 (6222–6244) | 3.06 | 2.4 | 1.6 | 2191 (2174–2213) | 5.70 |
| 2 peers × 1 flow | up | 3784 (3778–3795) | 6.40 | 3.0 | 2.3 | 3732 (3731–3739) | 3.88 |
| 2 peers × 8 flows | down | 7292 (7268–7347) | 3.30 | 3.0 | 2.3 | 3935 (3929–3940) | 2.93 |
| 2 peers × 8 flows | up | 2834 (2834–2836) | 8.90 | 3.2 | 2.2 | 3220 (3214–3294) | 4.22 |
| 2 peers × 64 flows | down | 4750 (4380–4790) | 5.75 | 3.4 | 2.6 | 2856 (2836–2866) | 3.97 |
| 2 peers × 64 flows | up | 2338 (2313–2412) | 11.15 | 3.3 | 2.3 | 2041 (2019–2079) | 6.26 |

"Down" is sink → node → peer (the node encrypts); "up" is peer → node → sink (the node decrypts). The
pipeline-thread columns are the same binary with `TS_EXPERIMENT_PIPELINE_THREAD=1`.

Latency through the node (64-byte echo from the Go client via SOCKS, 300–1500 samples, p50/p90/p99 ms):
idle 0.75/0.95/1.11. During the 2 × 8 download load: default 3.69/12.0/13.6, pipeline thread 0.72/0.99/1.18
(but the pipeline-thread load ran at about half the throughput).

**What it says:**

1. **The node is fast but doesn't scale with flows.** Per-GB cost rises with the number of flows: download
   3.0 → 5.8 CPU-s/GB and upload 6.6 → 11.2 from 1 to 64 flows per peer, while aggregate throughput falls
   from the 8-flow peak. smoltcp's `poll_egress` and the netstack's blocked-command replay both walk every
   socket per poll, so per-packet cost grows with the socket count. Sharding (N stacks, n/N sockets each)
   and/or making those loops readiness-driven are the two fixes.
2. **Upload costs about twice download** (6.4–11 vs 3.0–5.8 CPU-s/GB). The receive path (`recvfrom` per
   datagram, decrypt, netstack ingress, host write) is the bigger target. There's no receive batching
   (`recvmsg_x`) yet.
3. **The default node tops out around 3.4 cores** (4 tokio workers), and both 8/64-flow loads are near that.
   More throughput needs more cores used efficiently, not just less work per packet.
4. **A single pipeline thread is a hard ceiling.** It is 93% busy on one download flow (profile
   `scratch/prof/stage0-pipeline-down1.txt`) and caps it at 1.6 Gbit/s, with 2.4× the CPU/GB (and 3.4× on
   the generator: smaller bursts, more wakeups per packet). It wins on CPU/GB for uploads and many flows,
   and on latency, which is the case for run-to-completion. One thread can't be the design, which matches
   D17. Keep it opt-in, don't make it a default.
5. Generator headroom: at most 2.6 generator cores alongside 3.4 node cores. That's below the machine
   total, but the E cores are slower, so the 6–7 Gbit/s download peaks are partly machine-bound.

**Bugs this harness found (all fixed, with regression tests, on `ts-exit/runner`, `perf-connected` and
`bench-scaling`):**

- FIN/Close dropped on a full netstack command channel (bounded at 32): a lost half-close left peers
  waiting for the idle timeout. 1 of 64 concurrent uploads hung (`47b4714`).
- A cancelled receive (`select!`/`timeout`) left its command queued, and it consumed the next datagram
  (`652b74b`).
- The UDP reaper killed a brand-new flow in its port's first millisecond, so the gate's held first datagram
  to a cold port lost its reply (`cbf9f8c`). This was a regression from my own 938bed4, and
  `gated_all_ports_answers_first_datagram_to_cold_port` had been failing whenever run alone.

A real Go client's throughput was unaffected by peer updates (hostname changes mid-download, no dip), so the
synthetic stalls I saw during setup (a new control session briefly reporting no endpoints) were harness
artifacts. The harness now holds one session per peer for the whole matrix.

## Next

- Live laptop/iPad confirmation of the current stable build (2 MiB buffers, connected sockets as default).
- Sharding stage 1 (exit-flow ownership across N netstacks), A/B'd on this harness against §4.
- Receive-side batching (`recvmsg_x`) and readiness-driven socket polling (the O(sockets) loops in §4).
