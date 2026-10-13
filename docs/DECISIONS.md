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
  for exit flows (`824f2d2`). Stricter than Go, which re-adds the host's own interface IPs.
- **F6 Cold-port loss**: all-port mode dropped the first SYN/datagram per new destination port (3 s smoltcp,
  ~1 s iOS RTO) and copied every packet into a raw socket (~27% throughput). **D8**: replace with an ingress
  `PortGate` (`417dd29`), not codex's reinjection/pinning idea. The gate holds the packet until the listener exists
  and needs no raw socket. Cold connect went from 3,032 ms to 27 ms (1 RTT) in the bench.
- **F7 Upstream bug**: `WakingPipeReceiver::try_recv` blocked (`99d4474`). Latent in production paths.
- **F8 Window limit**: no TCP autotuning in the smoltcp netstack. One flow ≈ buffer/RTT (35 Mbit/s at 40 ms with the
  256 KiB default). RSS tracks bytes in flight, not the configured buffer size (BENCHMARKS §1a). **D9**: keep the fork default for
  the live baseline, A/B 2 MiB and 4 MiB via `--tcp-buffer-size`. That's config, not a code patch, so the Phase 1
  "no perf patching" gate holds.
- **D10 UDP idle** (`af993f6`): refresh flows on replies too, use Go's 2 min / 30 s (port 53) timeouts, and don't
  reap a gated UDP port with live flows.
- **F9 macOS send path** (`bench/udp-send`): unconnected sendto 7.9 µs/pkt → connected 4.8 → +DF 3.3 →
  +sendmsg_x 1.9. The fork's magicsock uses unconnected `send_to` per packet, plus one buffer allocation, copy and
  channel hop per received packet. Phase 3A/3B target, after the live baseline.
- **Blockers (2) and (5)** remain. Codex sized (2) at ~half a day (patch in progress) and (5) at 1–3 days.
  Until fixed they're documented limitations, mitigated by a tailnet where the only exit-node users are your own devices.

- **F10 Codex series review** (`docs/codex-03-review.md`) found real lifecycle bugs in the first PortGate manager:
  stale Ready/Exited events across port re-opens, non-atomic reaping, detached listener tasks surviving
  cancellation, and Opening never timing out. Fixed in `9d21497` with per-port generations, an atomic Closing
  claim, JoinSet ownership and a 10 s open timeout, plus Codex's two probes as regression tests. IPv6 rejections are
  rate-limited (`d89780d`).
- **D11 Blocker 2 fixed** by Codex's patches (`83f6076`, `3aba14b`), reviewed and applied by Claude.
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
  eating data, and the UDP reaper killing first-millisecond flows (my af993f6 regression; it broke the
  cold-port gate for UDP). Cherry-picked to `ts-exit/runner` and `perf-connected`; the live node runs them.
- **D18 Pipeline thread stays opt-in and is not the design.** It saturates at 93% on one download flow
  (1.6 Gbit/s, 2.4× CPU/GB). It wins on upload/many-flow CPU and on latency, so per-shard
  run-to-completion is right, but it needs N threads. Next is sharding stage 1 per codex-10, measured on
  the §4 harness.
- **F18 Peer updates don't disturb a real client's flow** (Go client hostname change mid-download, no dip).
  The synthetic stalls were the harness's fresh control sessions briefly reporting no endpoints. Fixed in the
  harness (one `hold` session per peer per matrix, and the enrolled hostname is reported, so control doesn't
  rename the nodes).
- **F19 Codex re-rank (docs/codex-12-stage0-replan.md), confirmed by profile.** Codex found quadratic work:
  each replayed stream command validates its socket with a full `socket_set.iter().any(...)` scan
  (`get_socket_mut!`), O(blocked × sockets) per pump. Profiles at 2 × 64 flows
  (`scratch/prof/stage0-default-{down,up}-2x64.txt`): `process_one_cmd` self-time (the inlined scan) is
  13.3% of busy node CPU on download and 8.4% on upload, plus `pump_waiters` 2.2%. smoltcp's per-packet
  socket lookup (`process_tcp`) is only 0.8–1.8%. **D19**: build the cheap registry-based validation,
  close dedup and bounded orphan reaping first (codex, branch `ts-exit/netstack-scans`), then receive
  coalescing, then sharding stage 1. No smoltcp scheduler rewrite unless a post-fix profile justifies it.

## 2026-10-13

- **F20 Listener orphan bug (real-client impact).** A client RST during the TCP handshake sends smoltcp's
  half-open socket back to LISTEN. Accept dropped it from the queue but left it in the socket set as an
  untracked second listener, which then completed later handshakes that nobody accepted, so the client hung.
  Found by instrumenting the harness's intermittent "0 bytes after node restart" stall: the generator
  connected but `accept()` never returned. Handshake resets are routine (Happy Eyeballs, cancelled
  requests, network switches), so this hit real clients too. Fixed with a packet-level regression test
  (`1cfc018`), cherry-picked to `runner`/`perf-connected`. The live node runs it (`bin/exit_node-live3`).
  Four restart-then-matrix repros were clean (previously this failed in every fresh matrix).
- **F21 Netstack scan work (codex, `ts-exit/netstack-scans`).** Registry validation: 2 × 64 download
  5.75 → 5.37 CPU-s/GB. Targeted waiter replay (opt-in `TS_EXPERIMENT_TARGETED_REPLAY=1`, smoltcp wakers
  plus a 50 ms safety sweep): → 4.67 (−19% total, +14% throughput). Upload only 11.15 → 10.57 (−5%). Single
  flow within noise. Counters before targeted replay: 120 replay attempts per completion.
- **F22 Upload is syscall-bound per packet.** Profile (2 × 64 upload, targeted replay): unbatched `sendto` of
  ACKs ≈17% and `sendmsg_x` 4% of busy CPU, `recvfrom` 9%, alloc/copy 14%, netstack 18%. **D20**: next is
  codex's receive/ACK batching (C1/C2: portable coalescing, `recvmsg_x`/`recvmmsg`, ACK send batching),
  then sharding stage 1. Targeted replay stays opt-in until a live laptop/iPad soak.
- **F23 Upload profile correction (codex):** 2,631 of 2,644 `__sendto` samples were host TCP writes to the
  sink, not ACK sends. **Brief 15 A/B** (one binary, targeted replay on): the copy/alloc/tracing cleanup is
  a clean win (2 × 64 upload 10.57 → 9.83 CPU-s/GB, +9%). RX batching (`recvmsg_x`/`recvmmsg`) gives another
  −17% (→ 8.14, +11% throughput; −27% upload CPU vs the stage-0 baseline). But either experiment flag also
  enables a 128-item work budget plus yields, which costs single-flow download ~65% CPU/GB and makes 2 × 64
  download unfair. ACK batching alone is neutral. **D21**: decouple the budget from the flags (codex brief
  16) and re-measure RX batching alone. Loopback-sink attribution was checked and rejected: the sink pays
  for its own receives (5.8 s per 6 GB received vs 0.5 s per 6 GB sent), so the up/down asymmetry is real
  node cost.
- **F24 Budget decoupled (codex-16).** The full work budget stopped after one egress pass, so a single TCP
  socket sent one segment per poll. RX batching now keeps only receive-side bounds. A/B: RX alone 2 × 64
  upload 9.75 → 7.97 CPU-s/GB (+14% throughput) with download unaffected (1 flow 2.86, 4.17 Gbit/s); the
  budget control reproduces the regression. Cumulative vs stage 0 in BENCHMARKS §5. **D22**: targeted replay
  and RX batching are the candidate defaults, pending a live laptop/iPad soak. Next build is sharding
  stage 1 on top of `ts-exit/netstack-scans`.
- **F25 Sharding stage 1 (codex, `ts-exit/shard-stage1`) A/B.** N=1 ≈ previous build (default path intact),
  no stalls. Only 2 × 64 download improves (N=4: 4.54 → 3.11 CPU-s/GB, 5.06 → 5.68 Gbit/s). Upload CPU/GB
  roughly doubles by N=8 (2 × 64: 7.88 → 15.36, 3.02 → 2.03 Gbit/s), and single-flow download drops
  4.30 → 3.3 Gbit/s at N≥2. Profile (N=4 upload): host TCP write syscalls up ~60% at lower throughput,
  i.e. smaller writes, since each shard writes each segment as it arrives. Single-flow loss is likely the extra
  hop's latency on a window-limited flow. **D23**: sharding stays opt-in and off. Next is coalesced splice
  writes (a general win, also at N=1) and a check on the hop latency (codex brief 18) before stage 2/3.
- **F26 Splice coalescing (codex-18, `TS_EXPERIMENT_SPLICE_COALESCE=1`) at N=1 is a clear download win:**
  1-flow download 2.79 → 1.92 CPU-s/GB (−31%), 2 × 8 −25%, 2 × 64 −12% with +15% throughput. Upload is neutral.
  At N=4 it doesn't fix sharded upload (2 × 64 up 13.7 CPU-s/GB vs 7.7 at N=1). An attribution run shows
  essentially no drops (1 drop in 4.2 M shard packets), so the N>1 upload cost is cross-thread overhead
  (wakes, the shared WG handoff, small per-shard batches), not loss or write size. Profile:
  `scratch/prof/co4-up-metrics.txt`. **D24**: coalescing joins the candidate defaults (with targeted
  replay and RX batching, pending a live soak). Sharding stays off. Its upload overhead must be profiled
  and fixed before stage 2/3 work.
- **D25 Consolidate + soak, and keep sharding going (user, 2026-10-13).** `ts-exit/consolidated` (e59e731) makes
  targeted replay, RX batching and splice coalescing default-on (opt out with `=0`). 1,109 tests pass both
  ways, and harness one-flow results match the coalesce run exactly. **It runs as the live node**
  (`bin/exit_node-consolidated`, smoke 6/6) with `runner/soak.sh` logging egress/HTTPS/RSS/CPU every
  30 min to `state/soak.log`. Codex brief 19 is attributing sharded upload overhead in parallel. Upstream
  survey in `docs/UPSTREAM.md`: nobody has done exit-node scaling. The official repo already fixed the
  listener orphan but still has the FIN-drop, abandoned-receive and global-replay issues. The Go
  counterpart of our macOS batching is tailscale/tailscale#21487 (open).
- **F27 Shard overhead attribution (codex-19, `TS_EXPERIMENT_SHARD_BATCH`).** Codex attributed 417/503 sampled
  mutex waits to a per-packet ownership-ledger lock, plus per-packet shard publications and a full shared WG
  mailbox. It added four opt-in treatments (batched dispatch, per-shard ownership cache, ready WG merging,
  inline egress); ownership locks went 352,115 → 3 in its probe. On the real harness they don't fix upload:
  2 × 64 up N=1 7.82, N=4 13.53, N=4+batch 13.82 CPU-s/GB. They help 2 × 8 download throughput (6.42 →
  7.73 Gbit/s). Codex's in-memory control with the generator's driver policy shows no N=4 single-flow
  download gap, and the real generator sits near one core at N=4. So the single-flow loss is likely a
  generator/testbed limit, not the node's. Generator + node + sink use ~6–7 of 10 cores (6 are E-cores), and
  macOS scheduling of extra threads onto E-cores is a likely confounder for the upload CPU/GB as well.
  Codex was cut off twice by model-capacity errors; Claude verified and committed the remainder (41631e2).
  **D26**: sharding is not evaluable further on this single Mac. Next evidence needs a remote generator
  and/or a many-core Linux host. Until then sharding stays opt-in and off, and the consolidated N=1 build
  is the product.
- **F28 Linux results (kworkstation, Ryzen 9 9950X3D, node pinned to 4 or 8 physical cores of one CCD, generator
  and sink on the other).** Sharding hurts many-flow upload on Linux too (2 × 64 upload, 8 cores:
  N=1 3.39 Gbit/s → N=8 1.44), so it isn't just Mac contention. Per-feature attribution (W=8, N=1, two passes):
  splice coalescing caps a single download flow at ~4.1 Gbit/s (−14% vs off, while saving 18% CPU/GB). RX
  batching costs 18–27% on many-flow uploads (and 8–10% on downloads when alone). Targeted replay is neutral to
  positive (+17% on 2 × 64 download). With everything on, the node used ~2 cores vs ~3 with everything off: more
  efficient but more serialized. **D27**: `ts-exit/consolidated` 3e9ee27 makes RX batching and coalescing
  default-on only on Apple; targeted replay stays on everywhere; `=1`/`=0` force either way. The Mac is
  unchanged (primary objective). Root-cause fixes so both become wins on Linux too (parallel decrypt after
  batched receive; bounded coalescing hold) are queued for codex.
- **Privacy scrub (2026-10-13).** A home public IP was published in a benchmark CSV (workspace) and as unit-test
  sample data (a9b583f, `ts_magicsock/src/path.rs`). All 9 `ts-exit/*` branches were rewritten with
  git-filter-repo (IP → 203.0.113.34) and force-pushed with Kasper's approval; doc hash references were remapped.
  LAN/tailnet addresses were judged fine (Kasper). GitHub may keep the old commits reachable by SHA until GC.
  Rule from now on: never commit public IPs; use documentation ranges in tests.
