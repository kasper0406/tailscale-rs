# Agent brief: Rust Tailscale on macOS M4 — exit node and throughput optimization

**Date:** 2026-10-11  
**Target:** Always-on Apple Mac mini M4 (Apple Silicon, macOS), serving as a home internet exit node for the **official Tailscale iPad client**.  
**End goal:** A locally built Rust-based Tailscale exit-node experiment; before/after reproducible benchmarks; small, reviewable changes suitable for possible upstream contribution.

## Objective and non-goals

Build and run a **server-side** Tailscale exit node using Rust. Route a remote Tailscale client's internet traffic through the Mac mini's residential connection, initially over IPv4 and eventually IPv6, without needing a Rust client app on the iPad. Compare baseline behavior with individual optimizations and quantify both tunnel and end-to-end exit-node throughput/latency.

- **Do not** reimplement a Tailscale iPad/macOS GUI client, public DERP relay, coordination server, or all Go daemon features.
- **Do not** disrupt the existing ASUS router VPN or normal Mac networking; keep the first iteration userspace-only and reversibly isolated.
- **Do not** claim production security or privacy guarantees for experimental Rust networking code. Testing is on a private, access-controlled tailnet.
- **Do not** benchmark only a host-to-host WireGuard tunnel and claim the numbers represent exit-node forwarding.

## Repositories to inspect (pin commits)

1. Official Rust library: https://github.com/tailscale/tailscale-rs — experimental, macOS ARM64 supported; as documented 2026-10-11, **exit nodes not implemented**. Architecture: https://github.com/tailscale/tailscale-rs/blob/main/ARCHITECTURE.md
2. Existing Rust fork: https://github.com/GeiserX/tailscale-rs — claims exit-node/subnet routing and contains `ts_forwarder`, `ts_transport_tun`, `ts_overlay_router`, `ts_host_net` in its source tree. Documentation: https://github.com/GeiserX/tailscale-rs/blob/main/PORTING.md and https://github.com/GeiserX/tailscale-rs/blob/main/docs/PARITY_ROADMAP.md . This fork explicitly warns it is unaudited and not production safe. Verify code and behavior; do not trust docs alone.
3. Optional daemon layer: https://github.com/GeiserX/tailscaled-rs — optional `tailnetd` and CLI wrapper. Its documentation is experimental and may lag current code; do not assume its exit-node and launchd support is functional. Prefer a small purpose-built runner until verified.
4. Reference Go implementation: https://github.com/tailscale/tailscale — only as a correctness/reference implementation, especially route advertisement, packet filtering, and macOS userspace exit-node forwarding.
5. Relevant Go macOS performance work, for conceptual reference rather than direct cherry-picks: https://github.com/tailscale/tailscale/issues/21602 ; https://github.com/tailscale/tailscale/pull/21603 ; https://github.com/tailscale/wireguard-go/pull/107 ; https://github.com/tailscale/tailscale/issues/21651 . These are Go-specific changes: reproduce the same *mechanisms*, not Go patches.

On setup, record for each repository `git remote -v`, full `git rev-parse HEAD`, branch, Rust toolchain, macOS build, architecture, and features. Document any changes since this brief.

## Architecture: minimal server-side exit node

```
Official Tailscale client (iPad / test laptop)
       | encrypted WireGuard (direct preferred)
       v
Rust Tailscale engine on Mac mini
       | decrypt / authenticate peer / apply ACL+packet filter
       v
Forwarding engine (IPv4 MVP)
       | TCP proxy, UDP NAT/forwarder, DNS and optionally ICMP
       v
macOS regular outbound sockets
       |
       v
Home router -> home ISP -> internet
       ^
       | reverse path through forwarding engine and WireGuard
```

*The Mac's existing home router already performs the public-IP NAT.* A userspace forwarder may use ordinary host sockets, so a first version does **not** require `utun`, macOS kernel forwarding or `pf`. The userspace engine must correctly restore response packets to the original tailnet client. A separate advanced experiment can later investigate a macOS `utun` + kernel-forwarding + `pf` NAT backend; root privileges and route/firewall lifecycle are major additional costs.

## Phase 0: establish baseline and prerequisites

- Clone the official Rust library, existing Rust fork and optional daemon repo in separate directories; read their `AGENTS.md`, `ARCHITECTURE.md`, `SECURITY.md`, `CONTRIBUTING.md` and actual Cargo workspace, preserving pinned SHAs.
- Confirm how to run a Rust Tailscale *node* that is visible to an official Tailscale iPad client. The official library is an embeddable library, not necessarily a drop-in `tailscaled` binary. Create a minimal launcher if needed.
- Build `aarch64-apple-darwin` in **release mode** with current supported Rust toolchain. Keep binaries and state directories distinct from the installed official Tailscale app; do not operate two full TUN/VPN clients at the same time.
- Arrange tailnet auth with least privilege; do not place long-lived auth keys in source, logs, or shell history. Require admin approval for advertised default routes, if control-plane policy demands it.
- Test `tailscale ping` / direct peer connectivity with the conventional Go client where possible, and determine DERP versus direct path. A relayed connection must be explicitly flagged in results.
- Benchmark official Go Tailscale on the same Mac as a reference exit node *if feasible* before Rust modification. Preserve configuration and run sequentially, not concurrently.

**Deliverables:** `docs/baseline.md`, immutable commit SHAs, build instructions, reproducible baseline, proof of direct vs relay path.

## Phase 1: validate the already-implemented Rust forwarder

The fork's docs describe `ts_forwarder` as the counterpart of Go `wgengine/netstack` exit-node forwarding. Inspect and test these specific seams:

1. **Route advertisement:** Does the Rust node advertise `0.0.0.0/0` for *being* an exit node? Do not confuse this with *using* an exit node (`Config::exit_node`, `Device::set_exit_node`). Is approval in the Tailscale admin console supported and reflected in the netmap?
2. **Peer policy:** Are incoming packets associated with the authenticated peer, checked against allowed source addresses and the effective tailnet packet-filter rules, and rejected unless authorized? Test against forged source addresses and an unauthorized peer.
3. **Packet delivery:** Does the Rust runtime pass packets not destined for the Rust node to the forwarder and return rewritten replies to the correct peer?
4. **TCP:** Confirm independent TCP sessions to external IPv4 hosts; connection closure, half-close, RST, retransmission and concurrent flows.
5. **UDP:** Confirm DNS and ordinary UDP request/response; flow keying, NAT mapping expiration, fragmented packets, backpressure; QUIC is particularly relevant for streaming.
6. **DNS:** Validate DNS egress without bypassing intended tailnet policy. Avoid assuming a DNS lookup succeeding means all internet routes work.
7. **IPv6:** If unsupported, either implement it or ensure clients cannot leak or blackhole IPv6 traffic. Do not advertise `::/0` until working. Test native dual-stack client behavior.
8. **Lifecycle:** Handle reconnection, rotating peer keys, control-plane updates, network changes, interface disconnection, and graceful shutdown.

Make the fork perform an **actual** user-visible test: official iPad Tailscale selects Rust Mac as exit node, a browser displays the **home public IP**, external HTTPS video can play, DNS operates, and a second test laptop can run controlled throughput tests. If the existing fork cannot do this, record precise missing functions and faults before porting anything.

**Gate:** No performance patching yet. A working, correct IPv4 baseline is required before optimizing.

## Phase 2: choose upstream integration strategy

Use two tracks:

**Track A (fastest proof-of-concept):** Maintain the existing Rust fork as the functional reference; get a private exit-node test running on Mac M4; benchmark before/after its internal optimizations.

**Track B (contribution-friendly):** Maintain an independent branch from **official** `tailscale/tailscale-rs`. Bring over only modular, tested components from the fork, with attribution and retained licenses, splitting changes into PR-sized patches:

- Route-advertisement API (`advertise_exit_node`) and capabilities.
- Forwarding trait / peer packet ingress / egress, behind a feature flag.
- TCP user-space forwarder with flow ownership and backpressure.
- UDP NAT mapping, timeouts and checksum handling.
- macOS userspace outbound socket backend.
- Security tests, DNS/IPv6 handling, metrics and recovery.

Before investing deeply in Track B, check official maintainers' stated priorities and contribution policy; do not assume a giant unreviewed port would be accepted. External PR creation may be restricted to collaborators.

## Phase 3: high-value macOS networking optimizations

Make every optimization independently switchable and benchmarkable. Start by profiling the tested Rust baseline. Potential experiments:

A. **Connected UDP sockets per active peer or path.** Measure whether `connect()` on a UDP socket improves send/receive overhead versus one shared unconnected socket. Preserve NAT roaming, peer endpoint changes and fallback; add bounded socket pool and cleanup. Check integration with DERP fallback.

B. **Darwin packet batching.** Investigate `sendmsg_x` / `recvmsg_x` and socket options where technically and legally practical. Evaluate compatibility across supported macOS versions and `unsafe` ABI correctness; retain safe standard socket fallback. Microbenchmark syscall cost / packets per second and full tunnel throughput.

C. **Datagram layout / MTU / fragmentation.** Experiment with IPv4 Don't Fragment behavior, PMTU, receive batching, buffer sizing, segmentation and checksum options; avoid fragmentation or broken IPv6/UDP behavior. Log path MTU and errors.

D. **Crypto and allocation profile.** Trace ChaCha20-Poly1305 time, copies, allocation rate, wake-ups, contention and packet parsing; try pool/zero-copy designs only where evidence supports them. Rust is not automatically faster than Go.

E. **Forwarder profile.** TCP proxy overhead, `smoltcp` polling, timeout scanning, per-flow state and memory; parallel flow handling, bounded queues and sharding if single shared network stack becomes limiting. Keep access control and connection semantics intact.

F. **Advanced optional track:** kernel-assisted `utun` + IP forwarding + `pf` NAT backend on macOS, isolated behind a feature gate. Never automatically modify system routing or firewall state. Explicit apply/rollback scripts, root-privilege checks, and no impact on management access required. Only pursue if userspace forwarding is demonstrated to be a significant bottleneck and simpler fixes are exhausted.

## Phase 4: reproducible benchmarks

Metrics: throughput Mbps (1 TCP stream, 4/8 streams, UDP packets/sec, UDP loss), idle RTT ms, loaded RTT ms, jitter, CPU time and peak RSS, allocations, effective connection path (direct/DERP), packet drops/retransmissions, 30–60 minute stability and network-rebind/rekey behavior. Run each scenario >= 3 times, report median and spread with fixed version SHAs.

A/B configurations:

| Configuration | Rust forwarding | Mac optimizations | Purpose |
|---|---|---|---|
| Official Go Tailscale exit node | Go baseline | Stock | Functional/performance reference |
| Rust fork baseline | Userspace | Stock | First working Rust baseline |
| Rust official+ported baseline | Userspace | Stock | Cost of upstream integration |
| Rust + UDP connected sockets | Userspace | Connected | UDP optimization |
| Rust + UDP socket batch | Userspace | Batched | Batching optimization |
| Rust + forwarding improvements | Userspace | Above plus forwarding | Exit-node bottleneck |
| Rust + kernel backend (optional) | Kernel-assisted | Above | Large architectural change |

Separate **tunnel-only** from **full exit-node** tests:

- Tunnel-only: test laptop -> Rust tailnet service on Mac; does not exercise internet forwarding.
- End-to-end egress: remote laptop through Rust Mac exit node -> *independent public test server* running `iperf3` (or a controlled, geographically stable HTTPS endpoint), checking public IP is the home connection; never test loopback and call it WAN forwarding.
- Streaming: iPad with official client -> Mac exit node -> F1 TV; compare connection stability over > one hour and playback recovery.
- Capture home upload/download and remote connection as environmental throughput ceilings. No amount of VPN optimization can exceed them.
- Test direct and DERP separately; no optimization claim is valid if peer path changed between runs.

Report raw results as CSV/JSON, commands, macOS versions, compiler options, network topology and objective interpretation. Separate improvements in tunnel packet rate from improvements in exit-node throughput.

## Security / rollback requirements

- Keep ASUS VPN as the fallback. Do not remove working remote access during experiments.
- Use a test tailnet or explicitly restricted devices and short-lived authorization credentials.
- Do not advertise routes until peer/source validation and packet filtering are tested. An exit node is a general internet egress, so access must be limited.
- All public-IPv4 and IPv6 packets must be handled intentionally, not silently routed outside the expected security policy.
- Don't change Mac default gateway, DNS or host packet filter as a side effect of the userspace baseline.
- Keep custom binaries, logs and state in an experimental directory; retain a single-command stop and rollback path that does not alter the official Tailscale app.
- Do not run as root unless specifically needed for the optional TUN/PF experiment, and do not run unreviewed experimental builds for sensitive/private traffic.

## Agent operating contract and output

1. Work in **small vertical slices** with tests, committing each feature separately; do not create one giant patch.
2. Investigate existing code before inventing new abstractions. Verify current APIs; no guessed function names or unsupported macOS socket flags.
3. Prefer functioning end-to-end tests over synthetic benchmarks. Maintain explicit acceptance gates.
4. Keep an issue/decision log: each missing feature, finding, source commit, tested scenario, known security caveat, experimental result.
5. At completion, deliver:
   - Buildable Rust release artifacts or exact reproducible commands for `aarch64-apple-darwin`.
   - `RUNBOOK.md`: setup, login, route approval, daemon start/stop, exit node activation on iPad, diagnostics, rollback.
   - `BENCHMARKS.md` with raw data and comparative results.
   - A patch series cleanly separated into correctness, forwarding, Mac UDP and performance optimizations, ready for upstream discussion.
   - Clear list of remaining unsupported features/security limitations.

## Recommended order

1. Fork audit and working Rust baseline on Mac.
2. Establish official Go exit-node control baseline.
3. Enable Rust as exit node **without optimizations** using fork code; verify iPad and remote laptop egress.
4. Port minimum necessary functionality into official branch if feasible and aligned with maintainers.
5. Connected sockets, then batching, then profiling-driven forwarder improvements, all with isolated A/B.
6. Optional kernel backend only if measurements justify complexity.
7. Seek maintainer review before proposing upstream submission.

**Success criterion:** An official Tailscale iPad client can use the Mac's **Rust** exit node for residential egress; correctness and fail-closed behavior are demonstrated; measurements isolate tunnel and forwarding improvements; every optimization is reversible and has evidence.
