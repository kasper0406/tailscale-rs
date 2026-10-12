# Decision / issue log

Newest last. Each entry: date, decision or finding, evidence, consequence.

## 2026-10-11

- **D1 Workspace.** All work lives in `/Volumes/git/ts-exit` (git repo; `repos/`, `state/`, `scratch/` ignored).
  Upstream clones are under `repos/` and pinned in `docs/baseline.md`.
- **D2 No Go baseline.** User declined installing Homebrew `tailscale` on the Mac, so the "Official Go Tailscale exit node"
  row of the A/B table is dropped. Comparisons are Rust-vs-Rust only. Environmental ceilings (ISP up/down, remote link) are
  still measured.
- **D3 Test peers.** iPad (official client) plus a remote test laptop (official client, outside the home LAN). No public VPS,
  so the WAN egress target has to be a public endpoint the laptop can reach through the exit node
  (see BENCHMARKS.md for which one and how its stability is controlled).
- **D4 Auth.** Tagged (`tag:rust-exit`), reusable, short-expiry key in `~/.config/ts-exit/authkey` (0600 in a 0700 dir).
  Read from the file only on first registration; never passed on argv or env, never logged.
- **F1 Home network has no global IPv6** (en0 has only `fe80::`; no IPv6 default route). IPv6 egress is impossible
  from this Mac regardless of software, so `::/0` must not be advertised. What remains is client-side behaviour:
  see the IPv6 section of `docs/phase1-validation.md`.
- **F2 Fork and official repo share no git history** (`git merge-base` is empty). The fork is a re-import plus 622 commits,
  so "cherry-pick" into official is not possible. Track B ports have to be manual, file by file.
- **F3 Official contribution policy** (CONTRIBUTING.md @ b44db0da): "Currently, we are not requesting external code or docs
  contributions ... contact the maintainers (e.g., by filing an issue) before sending a PR." DCO sign-off is required.
  So the right step is an issue/discussion first, not a PR. Track B is therefore kept to a patch series on a local branch.
- **D5 Runner.** Use a ~150-line purpose-built runner (`examples/exit_node` on fork branch `ts-exit/runner`) instead of
  `tailscaled-rs`. It needs nothing from the daemon layer (IPC, launchd, persisted prefs), and keeping it in-tree
  reuses the fork's `Cargo.lock`.

- **F4 Codex Phase 1 audit** (`docs/codex-audit-01.md`, verified by Claude where cited below). Five blockers:
  (1) HostExitDialer had no destination boundary; (2) node-key replacement keeps old WireGuard sessions;
  (3) IPv4-only advertisement hides the node from official clients; (4) the daemon layer never enables egress;
  (5) security state travels over a best-effort bus.
- **F5 Verified (3) in Go**: `tsaddr.ContainsExitRoutes` requires both `0.0.0.0/0` and `::/0`, and
  `peerStatusFromNode` sets `ExitNodeOption` from it. **D6**: deviate from PLAN Phase 1.7's "do not advertise
  `::/0` until working". Advertise the pair and reject IPv6 at the forwarder ingress (TCP RST / ICMPv6
  prohibited), matching what a Go exit node does when its IPv6 dial fails: RST. No enum/opt-out (codex proposed
  `ExitNodeIpv6::{Reject,Off}`). With rejection unconditional, the knob would guard nothing.
- **D7 Fixed (1)**: HostExitDialer refuses loopback/RFC1918/link-local/CGNAT/multicast/broadcast/0/8/class-E
  for exit flows (`a958192`). Stricter than Go, which re-adds the host's own interface IPs.
- **F6 Cold-port loss**: all-port mode dropped the first SYN/datagram per new destination port (3 s smoltcp,
  ~1 s iOS RTO) and copied every packet into a raw socket (~27% throughput). **D8**: replace with an ingress
  `PortGate` (`47d3cd2`), not codex's reinjection/pinning idea. The gate holds the packet until the listener exists
  and needs no raw socket. Cold connect went from 3,032 ms to 27 ms (1 RTT) in the bench.
- **F7 Upstream bug**: `WakingPipeReceiver::try_recv` blocked (`6ff66f4`). Latent in production paths.
- **F8 Window limit**: no TCP autotuning in the smoltcp netstack. One flow ≈ buffer/RTT (35 Mbit/s at 40 ms with the
  256 KiB default). RSS tracks bytes in flight, not the configured buffer size (BENCHMARKS §1a). **D9**: keep the fork default for
  the live baseline, A/B 2 MiB and 4 MiB via `--tcp-buffer-size`. That's config, not a code patch, so the Phase 1
  "no perf patching" gate holds.
- **D10 UDP idle** (`938bed4`): refresh flows on replies too, use Go's 2 min / 30 s (port 53) timeouts, and don't
  reap a gated UDP port with live flows.
- **F9 macOS send path** (`bench/udp-send`): unconnected sendto 7.9 µs/pkt → connected 4.8 → +DF 3.3 →
  +sendmsg_x 1.9. The fork's magicsock uses unconnected `send_to` per packet, plus one buffer allocation, copy and
  channel hop per received packet. Phase 3A/3B target, after the live baseline.
- **Blockers (2) and (5)** remain. Codex sized (2) at ~half a day (patch in progress) and (5) at 1–3 days.
  Until fixed they're documented limitations, mitigated by a tailnet where the only exit-node users are your own devices.

- **F10 Codex series review** (`docs/codex-03-review.md`) found real lifecycle bugs in the first PortGate manager:
  stale Ready/Exited events across port re-opens, non-atomic reaping, detached listener tasks surviving
  cancellation, and Opening never timing out. Fixed in `46a2402` with per-port generations, an atomic Closing
  claim, JoinSet ownership and a 10 s open timeout, plus Codex's two probes as regression tests. IPv6 rejections are
  rate-limited (`d3e10e4`).
- **D11 Blocker 2 fixed** by Codex's patches (`6bfbbee`, `681cd75`), reviewed and applied by Claude.
- **F11 macOS delivery check** (`bench/reuseport-check`): with `SO_REUSEPORT`, a socket connected to a peer on the
  shared port receives that peer's datagrams, other senders still reach the wildcard socket, and replies leave
  from the shared port (loopback and en0).
- **D12 Phase 3A prepared ahead of the live baseline** on a separate branch (`ts-exit/perf-connected`,
  `19e0507`). It's off by default with the Go knob names, so the gate "no perf patching before a working
  baseline" holds for the baseline binary: the same build with knobs off is byte-for-byte the runner behavior.
- **F12 Magicsock receive pump** ends permanently on any recv error (`DirectTransport::new`). Not hit by the
  shared unconnected socket in practice. The connected-socket pumps tolerate per-datagram errors (ECONNREFUSED).
- **D13 Code is saved** to github.com/kasper0406/tailscale-rs (a GitHub fork of tailscale/tailscale-rs, at the user's
  request): branches `ts-exit/runner`, `ts-exit/perf-connected`. They carry the GeiserX tree's unrelated history,
  so they don't compare against the fork's `main`. This workspace (docs/scripts) isn't pushed: the permission
  classifier blocked it, pending the user's decision.

## 2026-10-12

- **F13 Profiling** (macOS `sample` under the tunnel benchmark, `scratch/categorize.py`): spans at INFO on
  the packet path (16%), per-packet allocation/copies, tiny UDP buffers, a path-choice bug (hairpin), crypto,
  syscalls, and finally cross-thread wakeups.
- **D14 Optimizations kept**: trace-level spans, egress batching, 7 MiB socket buffers, Go `betterAddr`
  (all default, `ts-exit/runner`); `ring` AEAD (feature `ring-aead`, opt-in because ring's key schedule
  can't be zeroized); connected sockets + DF and `sendmsg_x` batching (env opt-in, `ts-exit/perf-connected`);
  runner `--worker-threads 2`.
- **D15 (revised) Worker cap 4, then a dedicated pipeline thread.** Capping tokio at 2 workers gave 30%
  on one flow, but with 8 concurrent flows it let per-flow tasks starve the serial packet pipeline (ingress
  drops, 3–4 s stalls). The user questioned the cap, rightly. 4 workers is safe and cheaper than 10. The real
  fix is to give the serial pipeline its own thread, so it can neither be starved nor bounce between
  cores. 1 worker (3.6 CPU-s/GB vs ~9–10) bounds the gain, so it's being built on `ts-exit/pipeline-thread`.
- **F14 `ring` and batching don't lower CPU/GB while the node is unsaturated** (the co-located Go client is
  the bottleneck at 5.2 cores), though each cuts its own share. They matter at saturation.
- **F15 Pipeline thread A/B** (same binary, both orders): −47% CPU/GB single download, −33% upload, −35%
  with 8 flows, but 8-flow aggregate −28% and more stalls, because the single pipeline thread saturates (86%).
  Codex's critique agreed with the isolation idea but not the mechanism or bound, and asked for a controlled
  A/B plus fairness budgets before any default. **D16**: keep it opt-in; candidate follow-ups are splitting
  the pipeline into two threads (underlay+crypto | netstacks) and bounded drains.
- **D17 Goal is general scalability, not this home line** (user, 2026-10-12). A stage/pipeline split is
  capped by its slowest stage and fixes the per-packet hop count, so it isn't the general answer. Direction:
  flow-sharded, run-to-completion threads (N shards, each owning its TCP-stack instance, per-flow host I/O,
  encryption and batched sends; receive steers by 5-tuple, at most one cross-thread hop; per-peer send
  counters atomic; control state shared read-only). Codex is critiquing the design before implementation.
  Design-independent pieces of the 2-thread work (bounded drivers, RX coalescing, DNS off the pipeline,
  flood-proof supervision, metrics, harness) are kept. A load generator that doesn't need the Go client
  comes first, because scaling beyond ~1.7 Gbit/s can't be shown on this Mac with the co-located Go client.
- **F16 Stage-0 baseline** (BENCHMARKS §4): two synthetic WireGuard peers through the real node to a
  loopback sink. Default node: 3.9 Gbit/s on one download flow (3.0 CPU-s/GB), 7.3 Gbit/s aggregate at 2 × 8
  flows, upload about twice the CPU/GB. CPU/GB grows with flow count (O(sockets) netstack loops), and the
  node saturates around 3.4 cores.
- **F17 Three netstack/forwarder bugs found by the harness**, all fixed with tests: dropped FIN/Close on a
  full command channel (channel now unbounded: requests wait for responses anyway), abandoned receives
  eating data, and the UDP reaper killing first-millisecond flows (my 938bed4 regression; it broke the
  cold-port gate for UDP). Cherry-picked to `ts-exit/runner` and `perf-connected`; the live node runs them.
- **D18 Pipeline thread stays opt-in and is not the design.** It saturates at 93% on one download flow
  (1.6 Gbit/s, 2.4× CPU/GB). It wins on upload/many-flow CPU and on latency, so per-shard
  run-to-completion is right, but it needs N threads. Next is sharding stage 1 per codex-10, measured on
  the §4 harness.
- **F18 Peer updates don't disturb a real client's flow** (Go client hostname change mid-download, no dip).
  The synthetic stalls were the harness's fresh control sessions briefly reporting no endpoints. Fixed in the
  harness (one `hold` session per peer per matrix, and the enrolled hostname is reported, so control doesn't
  rename the nodes).
