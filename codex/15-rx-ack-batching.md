# Brief 15: upload path batching (your C1/C2 from codex-12), opt-in

Worktree `/Volumes/git/ts-exit/scratch/wt-codex-scans`, branch `ts-exit/netstack-scans` (HEAD 12f705c,
now pushed; includes Claude's listener-orphan fix 12f705c). Continue there in new commits, same trailers,
`cargo +1.95.0`, format only touched files, no push.

## Evidence (Claude, harness §4, see docs/DECISIONS.md F20–F22)

Targeted replay brought 2×64 download to 4.67 CPU-s/GB (−19% vs baseline) but upload only
11.15 → 10.57. Upload profile with targeted replay (`scratch/prof/replay-up-2x64.txt`, 15.7k busy samples):
`__sendto` 2644 (≈17%: the node's ACKs back to the peer go out one datagram per syscall), `sendmsg_x` 627,
`__recvfrom` 1396 (9%), `chacha20_poly1305_open` 1285, memmove/free/malloc ≈14%, `__psynch_mutexwait` 517,
`core::fmt::write` 110 (find out what formats on the hot path).

## Build (each item its own commit, each opt-in or provably neutral, measured separately by Claude)

1. **ACK/egress send batching.** Find why most forwarder-netstack egress during upload reaches the underlay as
   single-datagram `sendto` rather than `sendmsg_x` batches (connected sockets + `TS_DEBUG_MAGICSOCK_BATCHED_IO=1`
   are on in the benchmark). Coalesce egress produced within one netstack poll / ingress batch into one
   underlay batch without adding latency (never wait to fill; flush at end of the current turn).
2. **Receive batching:** `recvmsg_x` on Apple for the connected-socket pump and the shared socket, with
   `recvmmsg` on Linux behind cfg, ordinary recv fallback; preserve source attribution, datagram boundaries,
   disco/STUN/relay demux, cancellation safety. Carry the batch through demux → decrypt → netstack ingress
   with one wake (your C1), without unbounded drains.
3. Remove avoidable per-packet allocation/copies on the receive path if cheap (reuse buffers), and any
   hot-path formatting/locks you find.

Gate env flags (e.g. `TS_EXPERIMENT_RX_BATCH=1`) where behavior changes; default unchanged. Tests: batch
boundaries, attribution of mixed-source batches, truncation, fallback on ENOSYS/EINVAL, cancellation, plus
the full suite from brief 13 in both modes, and `ts_forwarder --test forwarding` several times.

Write `docs/codex-15-rx-ack-batching.md` (commits, flags, tests, expected counters/effects, risks).
