# Brief 15: opt-in receive and ACK batching

2026-10-13. Worktree `/Volumes/git/ts-exit/scratch/wt-codex-scans`, branch
`ts-exit/netstack-scans`, based on `64bfe42` (including the listener-orphan fix).
New commits have the existing Signed-off-by and GPT-6.1 Co-Authored-By trailers.
No push. Claude measures each item separately.

**The upload profile's ACK attribution needs correcting.** In the supplied
`scratch/prof/replay-up-2x64.txt` in the parent checkout, the call-tree entries
for `__sendto` sum to exactly 2,644 samples:

| Caller | Samples | Meaning |
|---|---:|---|
| OS `TcpStream::poll_write`, under `copy_bidirectional` | 2,631 | Host TCP writes to the upstream sink. |
| `UdpSocket::send`, under magicsock `send_udp` | 13 | Single connected UDP sends. |

For example, profile lines 580–582 show 423 TCP write samples, and 3576–3578
show another 440. The syscall symbol alone does not identify UDP. Thus F22's
17% is overwhelmingly host TCP egress, and is not a 17% ACK-batching opportunity.
The 1,396 `__recvfrom` samples do sit under the connected receive pump. This
correction changes the expected benefit of item 1, not the requested opt-in scope.

| Commit | Isolated item |
|---|---|
| `34e41de` | Turn-boundary ACK/egress publication. |
| `22fc893` | Kernel receive batches, batch propagation and deferred ingress wakes. |
| `aad9b22` | Receive buffer ownership/reuse, fewer pipe locks and trace formatting. |

**Flags and default behavior.** Set these before startup. Neither experiment
is enabled by default; values must be exactly `1`.

| Flag | Effect |
|---|---|
| `TS_EXPERIMENT_ACK_BATCH=1` | Application and forwarder async netstacks publish egress in groups of at most 64, flushing partial groups before their poll returns. |
| `TS_EXPERIMENT_RX_BATCH=1` | Shared socket and connected pumps receive up to 16 datagrams per call, preserve the batch through direct demux/dispatch, and wake the ingress device once per admission chunk. Held port-gate releases also use one wake. |
| `TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1` | Existing Apple-only connected socket selection, required for the existing send batch syscall. |
| `TS_DEBUG_MAGICSOCK_BATCHED_IO=1` | Existing Apple-only `sendmsg_x` fast path for direct peer batches larger than one. |
| `TS_DEBUG_PIPELINE_METRICS=1` | Existing metrics gate, extended with receive batch counters. |

Either experiment also gives the async netstack a 128-item work budget and
makes runtime ingress/egress pumps yield cooperatively. Pipeline mode already
uses this budget. The connected receive pump and direct demux pump yield after
four batch turns. Receives never loop draining the kernel until empty in one
turn, and never wait to fill a batch. Targeted replay is independent; keep
`TS_EXPERIMENT_TARGETED_REPLAY=1` fixed for comparisons against Claude's replay
baseline.

The third item preserves packet behavior with both flags off. It does change
observability: `Peer::recv`'s span is now trace-level instead of the implicit
info level. An operator enabling trace still gets its session ID and batch size.

**Egress mechanism.** `send_wireguard_batch` already passes peer batches to
`sendmsg_x`, but explicitly skips the syscall when there is only one packet.
Connected sockets alone do not change that: `send_udp` then calls ordinary
`UdpSocket::send`, whose sampled Darwin syscall is `__sendto`. Before this work,
`TxToken::consume` published each ACK and woke its consumer immediately; the
consumer only coalesced whatever happened to be queued when it ran. On concurrent
workers it could observe a partial poll, and single-packet receive turns naturally
produce single ACKs even on one worker.

The opt-in device buffers emitted packets and publishes one channel message per
64-packet chunk. `IoPoller` calls the default-no-op device `finish_turn` hook before
returning either Ready or Pending; the pipe flushes its partial chunk there.
Receiver-side flattening retains the chunk across asynchronous receives, so the
existing 64-packet egress pump sees the published chunk together. Routing still
splits mixed-peer output by peer; a poll spanning several peers cannot become a
single connected-socket send. Retransmits, FINs and immediate ingress replies use
the same publication path. The sender's existing by-value TxToken implementation
is retained alongside the borrowing token needed to buffer device output.

**Receive mechanism and ownership.** Apple uses `recvmsg_x`, Linux uses
`recvmmsg`, both with `MSG_DONTWAIT`; other targets use ordinary Tokio receive.
The Apple header layout and truncation/partial-return rules follow
[XNU's socket_private.h](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/socket_private.h).
Linux's nonblocking behavior follows the
[recvmmsg manual](https://man7.org/linux/man-pages/man2/recvmmsg.2.html).
ENOSYS, EINVAL or EOPNOTSUPP disables the native batch syscall for that receiver,
then immediately tries ordinary receive. WouldBlock does not disable batching.
All FFI buffers and address/iovec/header storage outlive the synchronous syscall.

Each shared-socket message keeps its own IPv4/IPv6 source address. Connected
messages retain the socket's remote address. Lengths are capped to the same
1,600-byte prefix the original magicsock demux accepted; a truncated datagram
never includes the following datagram. STUN, Geneve relay disco/data, naked disco
and WireGuard keep their existing demux order and attribution rules. DERP transport
is unchanged. Batch grouping preserves each peer's packet sequence.

Raw tails and already-demultiplexed data live in the socket's receive state across
control-handler awaits. Cancellation retains those data packets. A control frame
already being handled has the existing consumption/side-effect semantics. If a
control handler fails, preceding data is delivered before its error is reported.
There is no await between consuming a kernel/channel batch and saving its tail.

Scratch receive buffers are retained by each pump, including the ordinary
1,600-byte buffer that used to be allocated on every `recv_data` call. One copy
from scratch creates an owned PacketMut; demux transfers it directly, and Geneve
payload extraction advances that buffer rather than copying. Single-packet bridge
and connected-pump messages use an enum instead of allocating a one-element Vec.
Published egress chunks are flattened with the Vec's owning iterator, avoiding a
second allocation for their descriptors. The default connected pump also copies
only the accepted 1,600-byte prefix of oversized datagrams.

Ingress still copies into Bytes for the netstack; TCP reassembly and host splice
copies remain. No buffer pool or zero-copy TCP rewrite was introduced.

**Formatting and locks.** Profile lines 57–60 trace the main formatting branch
through `Peer::recv` → tracing `new_span` → `DefaultVisitor::record_debug` →
`core::fmt::write` (86 samples there), with ANSI formatting and string growth below
it. Moving that span to trace removes its default-level per-receive formatting.
A call-tree classification of the 517 mutex-wait samples assigns 236 to command
requests, 83 to command receives, 141 to pipe queues, 42 to inlined pipe admission,
5 to socket lookup and 10 to other callers. These are sampled stacks, not elapsed
lock times. The queue/command synchronization remains. Pipe senders now cache the
immutable capacity kind, so unbounded transmit readiness avoids flume's capacity
mutex. Batch handoffs and deferred wakes amortize remaining queue/wake work;
bounded readiness and byte/packet refusal semantics remain.

**Validation.** All Cargo commands used `+1.95.0`. The expanded brief-13 suite
passed **1,028 tests across 27 test/doctest groups in each mode**, zero failures or
ignored tests, including all 597 runtime tests. It includes these packages:

```sh
cargo +1.95.0 test \
  -p geiserx_ts_netstack_smoltcp_core \
  -p geiserx_ts_netstack_smoltcp_socket \
  -p geiserx_ts_netstack_smoltcp \
  -p geiserx_ts_forwarder \
  -p geiserx_ts_runtime \
  -p geiserx_ts_magicsock \
  -p geiserx_ts_tunnel \
  --features geiserx_ts_runtime/ring-aead
```

The enabled run set both experiment flags, connected sockets, batched sends,
targeted replay and pipeline metrics to `1`. The default run left them unset.
Standalone forwarding integration stacks explicitly apply the experiment flags,
including the gated admission bridge.

- `ts_forwarder --test forwarding`: five additional consecutive runs per mode,
  16/16 each; plus ACK-only and RX-only runs, 16/16 each.
- Focused cases cover 63/64/65 egress boundaries, empty turns, both Ready and
  Pending poll flushes, cancellation after a Pending poll, deferred-wake counting,
  queue/byte refusal, 33 mixed-source datagrams including an oversized packet,
  zero-length packets, connected attribution, partial kernel returns,
  ENOSYS/EINVAL/EOPNOTSUPP fallback, WouldBlock, cancellation while empty and
  during a suspended control handler, ordered control errors, and buffer ownership.
- Existing STUN, disco, relay, source-filter, ABA, listener-orphan, forwarding
  half-close and fail-closed proxy tests pass in both modes.
- Linux library **and test code** cross-check passed for
  `x86_64-unknown-linux-gnu`. Rust's official standard-library archive was checksum
  verified and installed under `/tmp`; the host toolchain installation was unchanged.
  Linux runtime tests have not been run on this Apple host.
- Core `--lib --no-default-features` check and the exit-node example check passed.
- Targeted library Clippy with `-D warnings`, touched-file nightly rustfmt check,
  and `git diff --check` passed.

Logs are `/tmp/codex-15-all-{default,enabled}-final.log`,
`/tmp/codex-15-forwarding-{default,enabled}-{1,2,3,4,5}.log`,
`/tmp/codex-15-forwarding-{ack-only,rx-only}.log`,
`/tmp/codex-15-linux-final-check.log`, `/tmp/codex-15-no-std-check.log`,
`/tmp/codex-15-example-check.log` and `/tmp/codex-15-clippy.log`.
`bd prime` failed with `bd: command not found`; no tracker updates or sync occurred.

**Counters, expected effects and risks.** The exit-node cumulative metrics log now
includes `receive`. Its `kernel_batches[0..16]` counts successful native receive
calls by actual datagram count, excluding ordinary fallback. `fallbacks` counts
receiver instances disabling the native syscall, and `truncated` counts native
MSG_TRUNC results. Counters share the existing metrics gate and are relaxed,
process-wide cumulative samples. Existing app/forwarder `egress_batches`,
`sendmsg_x_offered_batches`, `sendmsg_x_batches` and `ingress_drops` remain.

For ACK batching expect larger forwarder egress and peer send batches where polls
actually emit several ACKs. This profile has only 13 ordinary UDP-send samples,
so no material CPU reduction follows from eliminating those samples alone.
For receive batching expect fewer native calls per packet and a receive histogram
moving above one; syscall work does not disappear just because a call carries
several packets. If the entire 8.9% receive-syscall share were fixed call overhead,
batches of 8–16 would remove roughly 7.8–8.3 percentage points of total busy CPU.
That is an optimistic bound, not a forecast. Copy/wake savings overlap it and must
not be added to earlier C1/C2 estimates. No performance benchmark was run here.

Compare `64bfe42` to item 1 with ACK batching alone, then item 2 with RX alone and
both flags, then item 3 with the same flags as its parent. Hold targeted replay,
topology, worker placement, buffers and other socket flags fixed. Keep metrics
attribution runs separate from metrics-disabled CPU/GB runs. Include upload and
download, one flow and 2×64, completion/EOF tails, matched-load latency,
retransmissions, ingress drops and memory. Reprofile host TCP writes separately
from UDP sends; ACK batching cannot eliminate the former.

The receive syscall is private on Apple and can fall back at runtime. Each active
native receiver retains 16×1,600 bytes of scratch storage, plus owned packets in
existing channels; those bridge channels remain unbounded as before. Poll-boundary
ACK publication changes wake timing only within the current bounded turn, but
can alter burst size and must be measured for latency/drop regressions. Mixed
peers and genuinely singleton turns still fragment batches. The explicit
`set_egress_batching` API is for the async driver; synchronous callers must leave
it disabled because that driver does not service the finish-turn hook.
