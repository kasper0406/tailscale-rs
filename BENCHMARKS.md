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
data is actually queued. Measuring RSS under many flows is still open; see "Next".
The runner exposes this as `--tcp-buffer-size`.

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
| unconnected `sendto` (**fork magicsock today**) | 184k / 184k / 200k | 8.5 / 8.5 / 8.0 |
| unconnected `sendto` + `IP_DONTFRAG` | 192k / 207k / 213k | 8.2 / 7.8 / 7.5 |
| connected `send` | 381k / 391k / 360k | 4.7 / 4.6 / 5.0 |
| connected `send` + `IP_DONTFRAG` | 716k / 712k / 536k | 2.7 / 2.7 / 3.6 |
| connected `sendmsg_x` ×32 + `IP_DONTFRAG` | 1,123k / 1,027k / 946k | 1.8 / 1.9 / 2.1 |

This matches the mechanism behind tailscale/tailscale#21602 (connected sockets + DF on darwin; DF lets macOS
skip the random IPv4 ID, and connecting saves the per-packet route/PCB lookup). It points Phase 3A (connected
per-peer underlay sockets) and 3B (`sendmsg_x`/`recvmsg_x`) at the fork's `ts_magicsock`, which today sends
every packet with `send_to` on one shared unconnected socket. Loopback is a lower bound on the effect: #21602
reports Macs in everyday use settling near 11k packets/s on the unconnected path.

---

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

## Next

- Live baselines (above).
- RSS vs `tcp_buffer_size` × concurrent flows (forwarder bench with many flows + `/usr/bin/time -l`).
- Phase 3A: connected per-peer underlay sockets + DF in `ts_magicsock`, with an A/B switch.
- Phase 3B: `sendmsg_x`/`recvmsg_x` batching behind a feature flag with a safe fallback.
- Loss/reorder injection in the forwarder bench to measure smoltcp's recovery.
