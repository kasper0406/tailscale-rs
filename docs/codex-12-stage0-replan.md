# Stage-0 measurements: revised scaling order

2026-10-12. Analysis only. Sources: [BENCHMARKS §4](../BENCHMARKS.md), its raw
[`tokio4-fixed`](../bench/results/stage0/tokio4-fixed/) and
[`pipeline1`](../bench/results/stage0/pipeline1/) JSONL, [F16–F18/D18](DECISIONS.md),
the [one-flow download profile](../scratch/prof/stage0-pipeline-down1.txt), and
`repos/geiserx-tailscale-rs` at `818deb2` on `ts-exit/bench-scaling`.
The measured node binary is documented at `4e6490d`; the relevant netstack,
runtime, magicsock and vendored smoltcp files have no differences between those
commits. All raw cells use the same node SHA-256. Source paths below are relative
to that checkout; `core/` abbreviates `ts_netstack_smoltcp_core/src/`.

**Build the cheap netstack fixes first; keep stage 1 as the next architectural
build.** Beyond F16's linear scans, command validation introduces quadratic
work. Isolate portable receive coalescing too: pipeline mode changes batching
and budgets as well as placement. Promote a full smoltcp scheduler/indexing
rewrite ahead of sharding only under the measurement criterion in D.

## A. Where the flow-count cost actually comes from

| Work | Source/function | Actual complexity and trigger |
|---|---|---|
| TCP packet lookup | `vendor/smoltcp/src/iface/interface/tcp.rs:31`, `InterfaceInner::process_tcp` | Linear search through TCP sockets until `accepts` matches, **per incoming TCP packet**, including ACKs. Average roughly half the set for uniformly active socket positions; worst case the entire set. Upload's data packets pay this; download's ACKs also pay it. |
| Egress | `vendor/smoltcp/src/iface/interface/mod.rs:506,700`, `poll_egress` → `socket_egress` | Walks socket storage and dispatches each eligible socket, stopping early if the device is exhausted. Not indexed by readiness. A pass can emit from many sockets; it is not necessarily one complete scan per transmitted packet. |
| Global waiter replay | `core/lib.rs:567,579`, `pump_waiters` / `pump_blocked_commands` | Replays every queued blocked request, not literally every socket. `IoPoller::poll` (`:692`) pumps on entry; ingress pumps after a changed drain (`:510`); egress pumps after progress (`:555`). The synchronous `poll_device_io` (`:328`) similarly repeats ingress/egress/pumps until quiescent. No replay budget in this loop. |
| **Nested validation** | `core/socket_impl/mod.rs:32`, `get_socket_mut!`; TCP Send/Recv/Close/ShutdownWrite in `core/socket_impl/tcp/stream.rs:17` | Every such command performs `socket_set.iter().any(...)` to check existence/type, then does an indexed `get_mut`. Thus replay of `k` blocked stream requests is **O(k × sockets)**, commonly quadratic when `k` tracks flows. Successful fresh Send/Recv commands also incur the scan. Generation checks alone do not remove it. |
| Listener/accept | `core/socket_impl/tcp/listener.rs:262,113`, `pump_tcp_accept` / `process_tcp_listen(Accept)` | Every socket-changing ingress packet pumps all listeners, allocating a listener-handle snapshot. Global waiter pumps do it again. Accept replay additionally scans that listener's half-open queue. The benchmark uses one destination port, so this is mostly a constant listener/allocation cost here; many ports or handshake churn make it scale. |
| Close/reap | `core/socket_impl/tcp/stream.rs:148,219`, `drain_tcp_closes` / `reap_orphaned_closed_tcp` | At an I/O poll that makes progress, pending closes each perform another existence/type scan: O(closing × sockets). The orphan pass runs even with no pending closes, rebuilding a listener-owned `BTreeSet` and scanning all sockets. Its pending-list membership check can add work for eligible Closed sockets. |
| Timer selection | `ts_netstack_smoltcp/src/run.rs:66` → `core/lib.rs:190` → `vendor/smoltcp/src/iface/interface/mod.rs:582,623`, `poll_delay` / `poll_at` | Every outer async driver turn finds the minimum deadline across sockets. Linear scan even if one socket changed. Socket `poll_at` and dispatch internals are private to smoltcp. |

The live driver uses `IoPoller`. “Poll per packet batch” is approximate: commands,
ingress, capacity and timers trigger polls; multiple egress passes need not
match actor batch boundaries. Only pipeline mode sets the 128-item work budget
(`ts_runtime/src/forwarder_actor.rs:189`, `netstack_actor.rs:219`). Storage scans
traverse vacant slots up to the allocation high-water mark
(`vendor/smoltcp/src/iface/socket_set.rs`, `items` / `items_mut`); live-flow count
understates cost after churn. Socket allocation also searches for a vacant slot.

**At 128 sockets:** one full egress, deadline or orphan scan visits 128 entries;
one uniformly distributed TCP lookup visits about 64.5. Replaying one blocked
stream command for each socket performs about **8,256 validation visits per
pump**, versus 128 indexed validations. Several pumps can occur per driver turn;
128 pending closes have similar validation cost. Listener/closing sockets add
slots beyond active flows. These are operation counts, not measured microseconds.
All-active egress can amortize its scan across many emitted packets; larger
batches do not remove ingress lookup or nested validation.

The raw medians confirm the regression. Holding peer count at two, 2 → 128
flows raises download CPU from **3.06 → 5.75 CPU-s/GB** and upload from
**6.40 → 11.15**. Removing *all* that excess would save 47% / 43%, respectively;
this is an optimistic attribution scenario, not proof that scans explain all
of it. Generator CPU/GB, bursts, scheduling, checksum/copy cost and cache pressure
also change. Download flow durations at 128 flows span
2.70–7.01 s across runs, so the active set shrinks during each finite transfer.

`python3 scratch/categorize.py scratch/prof/stage0-pipeline-down1.txt` reports
busy top-of-stack buckets: send syscalls 19.3%, netstack 14.6%, crypto 12.4%,
allocation/copy 11.6%, Tokio 7.9%, locks/wakes 7.1%, clocks 5.9%, receive syscalls
4.7%, tracing 2.1%. The pipeline thread is about 93% busy. This is a one-flow
download sample, includes other node threads, has inlined/uncategorized work,
and cannot attribute 128-flow scans or upload receive cost.

## B. Cheapest fix, and where smoltcp must change

1. **Remove validation scans first.** Extend the existing generation registry
   with socket kind, maintained by `core/lib.rs`'s sole `add_socket` /
   `remove_socket` wrappers (`:158,174`). Validate membership/type there before
   indexed access; the existing `BTreeMap` gives O(log n), without a smoltcp
   change. Use the same registry for pending-close checks, deduplicate closes,
   and maintain listener ownership instead of rebuilding it on each reap.
   Expect negligible single-flow improvement and **10–25% high-flow node CPU
   reduction if validation is a substantial share**; no supplied high-flow
   profile establishes that share. A tiny fallible indexed getter in vendored
   `SocketSet` is an alternative to the registry.
2. **Replay only affected waiters/listeners.** Keep blocked queues by
   `(handle, generation, operation)` and a deduplicated ready queue. smoltcp TCP
   and UDP already expose `register_recv_waker` / `register_send_waker`
   (`vendor/smoltcp/src/socket/tcp.rs:693,711`, `udp.rs:160,178`), behind its
   presently unselected `async` feature. One netstack waker per direction fans
   out to that socket's waiters; registrations are one-shot and must be renewed.
   Cover connect/state transitions, listener/half-open promotion, command changes,
   closure and cancellation. Schedule bounded orphan sweeps with a reclamation
   deadline. This can live in our layer. **5–15% high-flow CPU saving** is a
   hypothesis overlapping item 1's range; every shard benefits.
3. **Full ready egress, deadline scheduling and indexed ingress need smoltcp
   changes.** Its public egress API accepts the entire `SocketSet`; socket
   dispatch/deadline methods and ingress matching are internal. RX/TX wakers
   signal application I/O availability, not every reason TCP needs to emit an
   ACK, retransmit, zero-window probe, FIN or keepalive. A correct ready queue
   needs all those events, a deadline heap/timer wheel, device-capacity retries,
   and interface maintenance/fragment handling. Add exact-tuple ingress lookup
   with listener fallback as part of this work if per-packet matching dominates.
   Temporary socket sets or skipping polls on unchanged app readiness are unsafe
   substitutes. **20–40% total CPU saving at 128 flows** is a combined scan-work
   target only if profiling assigns it that much cost.

Preserve F17's lossless FIN/Close, abandoned-receive handling and generation/ABA
guard. Order multiple waiters, clean cancellation without depending on a future
socket wake, and bound work for fairness. Promote listeners before processing
the next SYN; delaying all accept work until batch end can lose connections.
Terminal sockets with unread RX must remain readable. Item 1 has much less risk.

## C. Upload: prioritize batch propagation over a crypto rewrite

Ranges below are **hypotheses for total node upload CPU/GB**, to be isolated by
A/B. They overlap and must not be summed.

| Order | Receive-side work | Expected effect and scope |
|---|---|---|
| 1 | Portable coalescing through demux, decrypt dispatch and ingress admission | **10–25%** target. `ts_runtime/src/direct.rs:1169` enables `DirectTransport::with_receive_batching` only with a packet runtime. Default `ts_magicsock/src/sock.rs:3011`, `DirectTransport::recv`, yields one datagram; pipeline drains up to 64 queued packets grouped by peer. Preserve batches through actors; change `pump_ingress_with_budget` (`ts_runtime/src/netstack_actor.rs:109`) / `WakingPipeSender::try_send` (`ts_netstack_smoltcp/src/pipe.rs:306`) to bounded batch admission with one wake. This reduces polls/scheduling, not per-packet tuple scans. |
| 2 | Kernel batching: `recvmsg_x` on Apple, `recvmmsg` on Linux, ordinary receive fallback | **5–15%** target from syscall/readiness amortization alone. `ts_magicsock/src/connected.rs:295`, `spawn_pump`, calls `sock.recv` per datagram; `sock.rs:1484`, `recv_data`, also handles wildcard RX singly. Cover both paths. Batches of 8–16 remove 87.5–93.75% of call count, not total CPU or kernel packet work. If calls consume fraction `r` of upload CPU, ideal call-overhead saving is roughly `r × (1−1/b)`. |
| 3 | Fewer receive allocations/copies | **5–15%** target, portable. Connected pump copies into a `Vec`; `recv_data` allocates a 1,600-byte buffer on each call, copies the connected `Vec` into it, then copies into `PacketMut`. Ingress copies again into `Bytes`; TCP Recv copies ring-buffer data for the host splice (`core/socket_impl/tcp/stream.rs:100`). Carry owned packet buffers through demux/admission and reuse receive storage. Pooling/lifetime complexity and unavoidable TCP reassembly/kernel copies limit the benefit. |
| 4 | Additional serial decrypt batching | **0–5%** alone. `ts_tunnel/src/endpoint.rs:1172`, `recv_from`, already groups by receiver ID; `session.rs:318`, `ReceiveSession::decrypt`, already loops over a batch and decrypts in place. Bigger upstream batches amortize grouping/dispatch, but each datagram still needs its own AEAD and replay decision. Parallel decrypt is stage 2 correctness work, useful for capacity once a measured owner saturates, not a cheap 2× CPU/GB fix. |

Default → pipeline saves **39%, 53%, 44%** upload CPU at two peers × 1/8/64 flows,
but changes coalescing, budgets and scheduling together. Isolate coalescing on
four workers first; the upload/download gap is not an AEAD or syscall attribution.
Batching must preserve source attribution, datagram boundaries/truncation, peer
FIFO, STUN/disco/relay demux and bounded cancellation-safe drains. Never wait to fill.

## D. Revised order and decision criterion

Ranked by general scalability benefit / (risk × effort); effect sizes are hypotheses.

| Rank | Work | Benefit / risk and effort |
|---|---|---|
| 1 | **B, small netstack fixes** | Remove quadratic validation, then targeted waiter/listener replay. Portable, survives sharding, small-to-medium effort; readiness has more risk than registry validation. Use the 10–25% high-flow CPU hypothesis above. |
| 2 | **C, portable receive batching/copy work** | Small-to-medium effort; attacks the expensive direction and existing per-datagram handoffs. Coalescing first, kernel batching by platform next. A Mac-only syscall fast path by itself does not outrank stage 1 for the general goal. |
| 3 | **Stage 1, exit-flow ownership** | First architectural build: parallel stacks plus colocated host I/O/listener/splice tasks. Medium-to-large effort and ownership/security risk. At 128 flows, a **roughly 1.3–2× aggregate throughput** target for 2–4 shards on unconstrained hardware is a hypothesis, bounded by the retained WG/underlay owner; no N-fold promise and no inherent single-flow gain from adding shards. |
| 4 | **B, complete smoltcp indexing/scheduler** | Removes dependence on idle sockets and remaining per-packet scans within every shard. Large effort/high timer and TCP correctness risk. Move above stage 1 only under the criterion below; do not block initial sharding on it. |
| 5 | **Stage 2, distributed WG transport data** | Unlocks capacity beyond the remaining serial crypto owner. Large effort/high replay, nonce, expiry/rekey risk. Ideal gain for fraction `f` made parallel is `1 / ((1−f)+f/N)`; e.g. **1.43× at N=4 if f=40%**. Measure `f` after stages above. The 12.4% one-flow crypto sample is not an aggregate owner ceiling estimate. |
| 6 | **Stage 3, underlay ownership** | Needed when socket drain/send owner or handoffs become limiting. Same conditional model: **1.18× at N=4 if f=20%**. Platform-dependent effort/risk; one outer peer socket may still bottleneck many inner flows. Rank can rise if profiles show socket ownership is the next ceiling. |
| 7 | **Stage 4, app services** | No expected benefit to this exit-to-host benchmark. Defer until app shard 0 or a real service workload needs it. |

Sharding makes stack execution parallel and changes host-I/O locality:
`ts_runtime/src/forwarder_actor.rs:46`, `spawn_forwarder`, currently uses ambient
`joinset.spawn`, and `ts_forwarder/src/tcp.rs:91` spawns each host splice there;
the isolated packet thread therefore does not colocate those splices. Sharding
does not remove dependence on socket count. With balanced ownership, a linear
scan costs n/N per event. Nested validation costs k n/N² per shard pump;
summing one pump from each of N shards gives k n/N. Savings per byte depend on
how pump frequency and batch sizes change, not just N. Listener replication,
uneven load and reduced bursts can offset gains.

For scale, assume *all* the two-peer 2 → 128-flow CPU excess were linear scan
work, batching stayed identical and four shards divided that excess by four.
Then download would move **5.75 → 3.73 CPU-s/GB (−35%)**, upload
**11.15 → 7.59 (−32%)**. This conditional model gives about 1.54× / 1.47×
throughput at the same CPU budget, before additional parallel capacity or
handoffs. It is an illustration, not a forecast from an attributed profile.

**Deciding measurement:** on the harness, keep the default topology and offered
PPS/bytes/s fixed below saturation; run 1, 16, 128 active flows in both directions,
then add 128/512 idle established sockets without changing active flows. Use
warmed, equal-duration windows. Profile default upload/download at 128 flows;
count slot/validation visits, replay attempts/completions, polls/packet, batches,
closes and drops. Record socket count/storage high-water mark. This separates
useless scans from useful TCP work and burst collapse.

- Ship the small B fixes first if they reduce 128-flow CPU/GB by **≥15%**, with
  **≤5% single-flow throughput loss**, preserved finite-transfer correctness and
  no matched-load latency/drop regression. These are proposed gates, not results.
- Put **full B before stage 1** if, after those fixes, remaining avoidable scans
  consume **≥25% of node busy CPU**, the idle-socket experiment reproduces the
  slope, and an isolated indexing/readiness experiment saves **≥20% CPU/GB**
  without the above regressions. Otherwise proceed to stage 1. A one-flow
  pipeline profile cannot satisfy this condition.
- For stage 1, compare old default and old pipeline against **new N=1**, then
  N=2/4/8, holding B/C features constant. N=1 isolates colocating splices;
  additional shards isolate ownership scaling. If aggregate throughput plateaus
  with one WG/underlay owner busy and shards underused, proceed to stage 2/3
  according to that owner's profile. Do not respond by adding more shards.

## E. What stage-1 A/B must add

- **Remote generators and sink, plus CPU accounting for all three.** Independent
  implementation is not independent hardware. All traffic currently originates
  and terminates on one Mac; its LAN endpoint can still traverse the local host
  networking path without a physical link. Four P and six E cores do not provide
  ten equivalent cores. CPU-s/GB is more useful than throughput here, but still
  changes with core placement, cache contention, burst size and drops. Record
  per-thread busy time, placement where observable, thermal state and generator
  headroom; repeat on a Linux multicore host. Co-location does not establish a
  portable throughput lower bound for different network paths.
- **Separate loopback, real egress and proxy cases.** The redirect still exercises
  netstack/forwarder/host TCP, but `ts_forwarder/src/dialer.rs:273`,
  `BenchRedirectDialer::dial_tcp`, invokes the inner dialer as `FlowClass::Subnet`
  to permit loopback. It bypasses normal exit destination handling; loopback
  lacks physical NIC/RTT/loss/offload behavior, and cannot validate real proxy
  exit behavior. Add controlled remote egress with nonzero RTT/loss and a
  separate configured-proxy case. Keep the 256 KiB buffer baseline and report
  any 2 MiB comparison separately; count memory per socket/shard.
- **Matched load and topology controls.** Current latency is 13.63 vs 1.18 ms
  p99 under about 7.29 vs 3.93 Gbit/s download, not equal load. The probe uses a
  Go client's SOCKS path, so report its owner/path separately from synthetic
  bulk exit flows. Measure fixed offered load below/near/above saturation,
  latency/fairness, queue occupancy, losses/retransmissions and offered/sent
  batch histograms. Equalize coalescing and work budgets across topology A/Bs.
  Include one peer × many flows, many peers, idle sockets, many destination
  ports, connection churn and an overloaded shard; two peers cannot establish
  the heavy-single-peer receive ceiling.
- **Longer windows, repeats and failures.** Current cells have three successes
  and only about 4–15 s of transfer, 30 MB per high-count flow, shrinking active
  sets and a completion tail. `runner/synthetic-matrix.py` excludes TCP setup
  and process teardown; upload waits for sink EOF, which correctly avoids
  treating buffered writes as delivery. Add warmed 30–60 s steady windows,
  cold-connect/churn runs separately, randomized paired order and uncertainty
  intervals. Sample process/thread CPU precisely and account for sink CPU if
  it shares a process. Preserve F18's stable control sessions and record all
  attempted runs: both raw directories contain a zero-byte setup timeout in
  `errors.log`, absent from success JSON. Per-peer `start_s` values have distinct
  process-relative origins; do not interpret their differences as GO skew.

