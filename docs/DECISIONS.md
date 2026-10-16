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
- **F29 Linux defaults verified (kworkstation, consolidated 3e9ee27, W=8, N=1, 2 passes).** Defaults (replay
  only) vs all-off: no regression on any load (all within ±1%); 2 × 8 download +4% and 2 × 64 download +21%
  (disjoint), 2 × 64 upload +18% (overlapping); RX batching confirmed inactive (`kernel_batches` all zero). The
  single-flow download cap is gone (4.69 vs 4.68 Gbit/s). Upstream issues filed: tailscale-rs#446, #447,
  plus a comment on #439 (docs/UPSTREAM.md).
- **F30 Linux fixes A/B (codex-20, `ts-exit/linux-fixes` 0a70e1b, kworkstation, replay on everywhere, 2 passes).**
  Codex showed WG decrypt was already serial with RX batching off. The Linux regressions came from bursts:
  large RX handoffs delay ACK admission, and 128 KiB coalesced download bursts. Fixes (Linux, flag-on only):
  ≤4-packet RX handoffs with yields; host→overlay reads passed on immediately in ≤16 KiB pieces. Vs the shipping
  Linux default (replay only): RX now +5.5% and −11% CPU/GB on 2 × 8 upload (was −18%) and −13% CPU/GB on 1-flow
  upload, but 2 × 8 download is still −6% (disjoint). Coalescing: 1-flow download −4.5% (was −14%) at −9% CPU/GB,
  2 × 8 download +3% with −6% CPU/GB, uploads worse but noisy. **D28**: keep both default-off on Linux (no pure
  win yet); keep the fixed Linux code paths (only active with the flags). Defaults on macOS unchanged.
- **D29 Branch reorganisation (codex-21, docs/REORG-21.md).** 97 commits → 36 signed-off commits, layered on
  the real GeiserX `main` (which also undoes the scrub's duplicated, unsigned GeiserX history): `fixes` (13) →
  `exit-node` (5) → `perf` (12) = `main` → `bench` (4) → `experiments` (2). Every layer builds, passes clippy/fmt
  and the full suite. `main` is equivalent to the old consolidated + Linux fixes minus experiments: on the Mac
  harness the old and new node binaries give identical results with the same generator (1-flow 2.87 vs 2.85
  Gbit/s down, 3.13 vs 3.14 up, same CPU/GB). The live node runs `main` (e909d0b), smoke 6/6, 520–540 Mbit/s
  through the node vs ~578 direct. Old branches are deleted after `archive/ts-exit/*` tags. Harness note: the node
  only probes a peer endpoint when it appears or changes in the netmap, and control remembers endpoints across
  sessions, so `stage0-matrix.sh` now picks fresh peer ports per matrix. Generator builds changed today, so only
  compare runs made with the same generator binary.
- **F31 Real-world laptop test (2026-10-13, laptop away from home, Wi-Fi, ~23 ms to the node, path verified
  direct on the node for every valid run).** Results in `bench/results/laptop/` (CPU logs) and below.
  | run | build | down / up Mbit/s | single flow | idle latency (TCP / TLS / HTTP ms) |
  |---|---|---|---|---|
  | A | `ts-exit/main` 51ee63d, 256 KiB | 91 / 105 | 9.0 MB/s | 61 (22 / 106 / 57) |
  | B | A + no Nagle (smoltcp + host `TCP_NODELAY`) | 97 / 66 | 8.7 MB/s | 46 (22 / 80 / 37) |
  | C | B + 2 MiB window | 58 / 193 | hung | 62; loaded HTTP 15 s |
  | D | C + smoltcp Cubic | 48 / 196 | 0.32 MB/s (stalled, node ~0% CPU) | 44; loaded HTTP 22 s |
  | direct (same session as D) | | 184 / 182 | 20.1 MB/s | 12 (6 / 27 / 4) |
  Findings: (1) **single flow is window-capped**: 256 KiB / ~29 ms ≈ 9 MB/s, exactly as measured. (2) **Larger
  windows break the node→client direction on a lossy path**: the smoltcp sender stalls (no SACK; the forwarder's
  smoltcp is built without any congestion controller, `NoControl`), and Cubic does not fix it, so the stall is in
  loss recovery, not cwnd. The client→node direction (macOS TCP sender) benefits from the bigger window. (3) **No
  Nagle** cut TLS by ~25 ms and HTTP by ~20 ms in one run each on a noisy link: plausible, not yet proven. (4)
  **Restart bug**: after a node restart an already-connected client stays on DERP from the node's side (the
  client's own `tailscale status` says direct, because its pings reach the node); quitting/reopening the client
  app (new netmap entry) makes it go direct. Reproduced with the committed build, so it is not caused by B–D.
  (5) Node CPU at real-world rates is ~20–30 CPU-s/GB (vs 1.9 at multi-Gbit in the harness): batching does not
  amortise at ~100 Mbit/s. Not a bottleneck (≤ 0.8 core), but worth a profile. The live node is back on A.
  (6) **Dropped half-open connections:** in run A2 (script run by the laptop session, 19:57 UTC), networkQuality's
  upload through the node failed with `errSSLClosedNoNotify`; at that moment the node logged `half-open socket in
  unexpected state, dropping` ×12 (`ts_netstack_smoltcp_core/src/socket_impl/tcp/listener.rs`, the Accept
  path's `_` arm, which doesn't log the state). Unexplained so far; needs the state logged.
  Run A2 numbers (laptop now ~230 Mbit/s): single flow 9.3 MB/s exit vs 30 MB/s direct; TLS 82 ms after connect
  via exit vs 21 ms direct; TTFB 175 vs 73 ms; networkQuality down 82 vs 230 Mbit/s.
- **D30 Follow-ups from F31 (2026-10-13).** codex-24 (`codex/24-tcp-recovery.md`): smoltcp sender loss recovery,
  reproduced offline with the new `BW_MBPS`/`QUEUE_KB` bottleneck in forward_throughput (branch
  `ts-exit/tcp-recovery` ac68a55; 150 Mbit/s, 25 ms: 256 KiB window + 64 KiB queue → 0.9 Mbit/s; 2 MiB + 256 KiB
  → 0.8 Mbit/s; Cubic the same; random loss ≤ 2% doesn't stall). Source shows a 1 s min RTO doubling to 60 s, no
  sender use of SACK, go-back-N after fast retransmit. codex-25 (`codex/25-restart-direct.md`): the restart bug.
  Nagle A/B continues on the laptop via its Remote Control session (Kasper approved the tests there).
- **F32 Nagle A/B on the laptop (2026-10-13, 4 runs by the laptop's Remote Control session, `run.sh`: 3 single
  100 MB flows, 5 HTTPS fetches of www.apple.com, networkQuality -s; exit then direct; runs A2 = `A-256k`, B =
  `B-nonagle`, B2, A3).** TLS time after connect through the node (mean of the 4 warm fetches): Nagle on (A2, A3)
  82 / 78 ms; Nagle off (B, B2) 60 / 59 ms, and 62 ms in Kasper's manual no-Nagle run earlier; direct 21–23 ms. TTFB
  improves ~15 ms. Single flow unchanged (window-capped, 8.8–9.8 MB/s vs 22–33 MB/s direct). B's erratic single
  flows (2.9–9.7 MB/s) coincided with three brief direct→DERP flaps of the node's path; B2 (one brief flap) was
  steady. The networkQuality upload through the node failed with `errSSLClosedNoNotify` on mensura `/gm/large` in
  A2 and B2 (both builds) and succeeded in A3 (244 Mbit/s, same as direct): intermittent, cause unknown; A3 ran
  with forwarder debug + listener trace logging and saw nothing. Verdict: adopt no-Nagle (smoltcp
  `set_nagle_enabled(false)` + host `TCP_NODELAY`, as Go does) after a LAN throughput/CPU check.
- **F33 Restart bug fixed (codex-25, 2026-10-13).** Root cause: `MagicSock::emit_disco_pings` returned on the
  first UDP send error after `send_pings` had stamped every candidate, so one unsendable candidate (likely the
  laptop's IPv6 endpoint on the IPv4-only socket) starved the rest on every sweep in a stable per-process order.
  Fix: send each candidate independently, as Go does. Live A/B with `runner/restart-direct-check.sh` (restart the
  node, time until the node's own status says `path=direct` for the idle laptop), alternating builds: old build
  TIMEOUT (> 120 s, still DERP) 2/2; fixed build direct in 4.1 / 3.0 / 3.1 s. Release builds compile out debug
  logs, so the failing candidate itself wasn't observed. Pre-fix, 7 of 7 restarts tonight left the laptop on
  DERP until its app restarted. Now the top commit of `ts-exit/fixes` (307bf6b; layers restacked: main b78d017);
  GeiserX PR #606. The live node runs it (`bin/exit_node-restartfix`). Note: `runner/env.sh` defaults
  `TS_EXIT_TAG=tag:rust-exit`, but the live node is untagged, so run the check with `TS_EXIT_TAG=`.
- **F34 TCP loss recovery (codex-24) + independent review (2026-10-14).** codex rewrote smoltcp's sender recovery
  (`docs/TCP-RECOVERY-24.md`): NewReno partial-ACK repair, a bounded 32-range SACK scoreboard, cwnd over bytes in
  flight, Reno/Cubic fixes, slow-start restart after RTO, RTT pacing, a 200 ms computed-RTO floor, 32 assembler
  holes, and an async-runner fix (stale I/O timestamps gave 0–20 ms RTT samples on a 25 ms path). Bottleneck bench
  (150 Mbit/s, 25 ms): every stalled configuration now 70–142 Mbit/s, longest pause 0.25 s (was 4–12 s); no-loss
  rows +4–16%. Recommends a 1 MiB window. The independent review (`docs/review-26-tcp-recovery.md`, Claude
  subagent) found **CRITICAL-1**: SACK blocks validated only by wrapping endpoint comparisons, so a SACK-negotiating
  downloader could make `Scoreboard::pipe()` loop forever (node wedged at 100% CPU). Fixed in 4e6b554 (accept
  blocks only inside the flight by wrapping offset from ack; bounded walks); re-check: 400k randomized rounds,
  ~2.15 billion segments, no panics or busy-loops (pre-fix harness hung in 538/2000 rounds). LOW-2 (pre-existing in
  smoltcp, reduced by the patch): no persist timer, so a zero-window or zero-window-then-reopen flow can stall
  until keepalive (≤ ~3 min, that flow only). Release builds run with overflow-checks off (MEDIUM-1, context).
  Live candidate `ts-exit/live-candidate` = main b78d017 + 14 production commits + no-Nagle (44c854d) + 4e6b554;
  running since 2026-10-14 with `--tcp-buffer-size 1048576` (`bin/exit_node-candidate`); smoke 6/6 at 593 Mbit/s.
- **F35 Candidate on the laptop (2026-10-14 ~00:00 UTC, runs C and C2 via `run-ts.sh`, no app restart: the restart
  fix kept the path direct).** Single flow through the node 16.9 / 18.8 / 19.4 and 20.0 / 7.6 / 20.0 MB/s (was
  capped at 8.8–9.8); direct 20.8 (noisy run) and 29.5–31 MB/s. The 7.6 MB/s flow coincided with the node's CPU
  dropping (no stall, path direct): a loss-recovery episode on Wi-Fi. networkQuality upload completed in both (158
  / 157 Mbit/s; it had failed in 2 of 4 earlier runs). TLS after connect ~60 ms (no-Nagle level). Loaded
  responsiveness: down 210 / 283 ms (A3 442), up 337 / 124 ms (A3 104), noisy. Node: ~18% CPU average, 66 MB RSS.
  Open: networkQuality's sequential download through the node is ~101 Mbit/s in A3, C and C2 regardless of build
  or window, while the forwarder alone does 190 (1 flow) / 217 (16 flows) Mbit/s over an emulated 250 Mbit/s, 25 ms
  link. A parallel-download test (`par.sh` on the laptop) is waiting for Kasper's approval in the laptop session.
- **D31 Layers after the merges and the candidate (2026-10-14).** Rebased onto GeiserX 650e0e2 (#602/#605/#606 merged
  upstream; our 5 copies dropped). The perf layer now also carries the TCP recovery (codex-24, 14 commits), the SACK
  fix 4e6b554 (kept separate) and no-Nagle; the bench layer gains the bottleneck/recovery-gap/latency-probe bench
  commits. fixes ca2a4ad (10) → exit-node 4b73115 (5) → perf 5719937 (28) = main → bench 3d76a28 (7) → experiments
  0528ebc (2). Workspace clippy -D warnings and all tests pass on experiments and on main. `main` equals the live
  candidate plus GeiserX's ruzstd and audit commits. Pushed to kasper0406/tailscale-rs. PRs #597/#598 on GeiserX
  still carry the old stack; not updated (the TCP work would be a scope change for #598; ask first).
- **F36 Stuck on DERP after a peer moves; listener drops (codex-27, 2026-10-14).** Live: the laptop slept ~04:54
  UTC and woke on a different Wi-Fi (easy NAT, `MappingVariesByDestIP: false`); the node stayed `path=derp` for
  3+ hours while the laptop said direct; run C3 through it was flat ~21 Mbit/s. Root cause: `recompute_best`
  scanned all historical RTTs, so the dead fast endpoint beat the fresh one forever, and our betterAddr guard
  then refused to re-trust it: permanent DERP. Fix (Go's `betterAddr || bestUntrusted` on the pong in hand) is in
  the fixes layer (857d2ea). Connected sockets were cleared. Listener: CLOSE-WAIT before accept was closed (request
  discarded), Closed sockets with unread RX leaked; fix on top of the perf layer (it conflicts with the perf
  layer's listener rework lower down), and the experiments layer's opt-in `accept_half_closed` flag was removed
  as redundant. Layers: fixes 857d2ea (11) → exit-node 304763b → perf 7089f9f (46) = main → bench 74e24f4 →
  experiments 5c77397; clippy/tests/fmt/checks pass. Live node restarted on main 7089f9f
  (`bin/exit_node-main-7089f9f`, 1 MiB); the laptop was asleep, so the fix isn't confirmed live yet. Smoke test
  now downloads from Apple's networkQuality CDN (Cloudflare and OVH rate-limited the home IP after the tests).
- **D32 Switch to GeiserX main (user, 2026-10-14).** GeiserX merged #597 (+ their exit-DNS fixes) and #598 (no
  Nagle, 7 MiB buffers, egress batching, socket registry, half-close accept, 32 holes on by default; our Apple
  RX batching, splice coalescing, targeted replay and connected sockets opt-in), and #599/#601/#603/#613/#614;
  #600/#604 were superseded (smoltcp 0.14 from crates.io; betterAddr inside #597). Not included: our vendored
  smoltcp TCP recovery (codex-24), which can't build on crates.io smoltcp. GeiserX also pins Reno (#627). Live now:
  GeiserX main 18fc6bc + `--serve-tcp` (0e10bc3, `ts-exit/main`), node renamed `kmini`, `ssh kmini` via Serve
  TCPForward to 127.0.0.1:22, default 256 KiB window, `TS_EXPERIMENT_*` off, Apple magicsock flags on. Smoke
  6/6; LAN single flow ~480–500 Mbit/s. Old layers archived as `archive/ts-exit/*-pre-geiserx-switch`.
  gpt-6.1-sol design review (`docs/review-28-design.md`): **go with changes**. Done before the switch:
  transactional `update.sh` (keeps `.prev`, health = log "running" + exit HTTPS fetch + SSH banner, auto
  rollback; it rolled back correctly on its first, mis-matched run), a rotator that never blocks the node and caps
  records, bounded `launchd.log`, explicit build artifact. Open from the review: Serve bind failures are only
  logged (fix in the GeiserX serve PR: bind before acknowledging, reject duplicate ports); ACL-restrict TCP/22 if
  the tailnet is shared; FileVault recovery path before any planned reboot; A/B the `TS_EXPERIMENT_*` flags and
  larger windows on this build; upstream the TCP recovery to smoltcp (port onto main, small PRs, keep the SACK
  validation fix), optionally carried meanwhile as a `[patch.crates-io]` on 0.14.
- **D33 Carry patched smoltcp 0.14 on GeiserX main (user, 2026-10-15).** smoltcp's CONTRIBUTING adopts the NLnet
  Labs LLM policy (human-authored code only; AI analysis and suggested fixes in bug reports allowed with
  disclosure), so upstream gets Kasper's rewrites of `docs/smoltcp-drafts/` B1–B3 (codex-29,
  `docs/SMOLTCP-UPSTREAM-29.md`: most of our 0.13 congestion fixes are already upstream; B1 failed-emit loses the
  fast retransmit, B2 zero-window reopen drops the RTO, B3 Cubic doesn't build without std; NewReno, sender SACK
  and pacing are features). Our build carries a pinned vendored smoltcp 0.14.0 + patches (codex-30,
  `ts-exit/tcp-0.14`, `docs/TCP-WORKAROUND-30.md`). Bottleneck matrix (25 ms, 150/250 Mbit/s, shallow queues): stock
  median 57 Mbit/s, max pause 2.03 s; no-patch mitigations (burst cap, Cubic, 1 s keepalive) don't help; patched
  + Cubic + pacing + 200 ms floor median 141 Mbit/s, max pause 0.23 s; 1 MiB single flow 121–230 Mbit/s vs ~69 at
  256 KiB. CPU-bound no-loss 8-flow 0 ms/4 MiB is ~9% lower (lab only). Independent review 31
  (`docs/review-31-tcp-0.14.md`): safe to run live, two LOW findings; its stronger harness (wraparound, failed
  emits, pacing, partial ACKs, poll_at-driven drain) passed 1M release + 100k overflow-checked rounds. LOW-1
  (pacing debt) fixed in 92c7b2e and rerun clean (500k). Live since 2026-10-15 via `update.sh` with
  `--tcp-buffer-size 1048576`; smoke 6/6 (503 Mbit/s on LAN). `ts-exit/main` = `ts-exit/tcp-0.14` (92c7b2e).
  Laptop validation (run D) requested.
- **F37 Real-world pacing A/B (2026-10-16, laptop on an away Wi-Fi network, ~50 Mbit/s access link, direct path).**
  Build `exit_node-pacing-live` (stock smoltcp 0.14.0 + runtime-switchable pacer, Reno, 1 MiB window); pacing
  switched by the Mac on 120 s UTC slots (`runner/pacing-schedule.sh`), laptop blocks clock-aligned
  (`deploy/laptop/run-pacing-sched.sh`; SSH from the laptop to kmini isn't key-authorized). Analysis:
  `runner/pacing-real-analyze.py`; data in `bench/results/pacing-real/`. Pacing engaged (≈260–330k deferrals per
  on-block vs 3–5 off) but **no measurable difference**: single flow median 21 (off) vs 22 (on) Mbit/s, 8 flows 49
  vs 48 (link cap), retransmissions 3.1% vs 3.0%, 13–21 RTOs per 30 s block in both. Interpretation: losses on this
  path aren't caused by our bursts (Wi-Fi or a policer), so pacing can't remove them; with stock recovery (1 s min
  RTO, no NewReno/SACK) the RTOs cap single flows at ~40% of the link. On this network loss recovery is what
  matters. For the upstream issue: present pacing for shallow-queue bottlenecks (emulated evidence,
  `docs/PACING-AB-32.md`), don't claim a real-world gain. Normal patched build restored afterwards (smoke 6/6).
