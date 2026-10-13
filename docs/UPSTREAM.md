# Upstream landscape and posting plan (2026-10-13)

## Has anyone done this already?

No. Searched `tailscale/tailscale-rs` (official, ~1.2k stars) and `GeiserX/tailscale-rs` (our base fork)
issues/PRs for exit node, throughput, performance, smoltcp, netstack, sharding, batching, benchmark, forwarder,
subnet router, scalability, listener/RST/half-open/FIN.

- **Official tailscale-rs can't act as an exit node or subnet router.** #405 tracks *using* an exit node
  (client side only). #444 proposes subnet-router support (advertise routes + raw-packet overlay transport,
  ESP32 use case), with no PR yet. Nobody has published exit-node scaling or multi-core work. The general
  performance tracking issue #14 was closed "in favor of more focused performance epics in the future".
- **Related upstream threads:**
  - #88 (open, danderson): "consider quinn-udp for high performance UDP stuff". Same goal as our
    `sendmsg_x`/`recvmsg_x` batching (§1b, §5).
  - #442 (open): a retransmitted SYN can hit the replacement listener and reset a valid connection. This is
    a close cousin of our F20; the same listener-replacement pattern plus smoltcp's first-match dispatch.
  - #28/#141 (closed): poor TCP accept rate, fixed.
  - #438 (merged): smoltcp 0.14 with congestion control on. Our fork is still on vendored 0.13.1 plus the RTO
    backport.
  - #398 (merged): TCP throughput dev server, TCP buffer to 64 KiB.
- **Status of our netstack bug fixes upstream** (official repo `main`, checked 2026-10-13):

  | Our fix | Official repo |
  |---|---|
  | F20 listener orphan (reset half-open left listening) | **Already fixed** (they close the socket in Accept). GeiserX lacks it. Ours also prunes on every listener pump. |
  | F17 FIN/Close dropped on a full command channel (`try_send` Full → Ok, bound 32) | **Still present** |
  | F17 abandoned blocked receive eats the next datagram | **Still present** (`pump_blocked_commands` replays without a disconnected check) |
  | F21 global blocked-command replay (120 attempts per completion) | **Still present** (same structure) |
  | UDP reaper first-millisecond (a55d5e2) | N/A (that was our own regression in the GeiserX-based code) |

  The F17 pair and targeted replay are small, self-contained, upstreamable fixes even before any scaling work.

## Go references to cite

- tailscale/tailscale#21487 (open, bradfitz, 2026-09-25): `net/msgx`, batched datagram syscalls on macOS via
  `recvmsg_x`/`sendmsg_x`. The Go counterpart of our Apple batching. tracking #21486.
- tailscale/tailscale#21020 (merged): ~128 KiB packet buffers for batched I/O (wireguard-go memory model).
- tailscale/tailscale#21119 (merged): size crypto and per-peer queues by CPU factor. Go's parallel crypto
  pipeline, the analogue of our stage 2. Its benchmark shape (TCP/1, TCP/128, netstack TX/RX side, peak RSS)
  matches our harness cells and is the format to report in.
- tailscale/tailscale#19054, #19888, #19918: Linux batching/GSO/GRO defaults and node caps.
- Earlier: #6808 (ReadBatch/WriteBatch on old kernels); Go's netstack forwarder timeouts (2 min UDP, 30 s
  DNS) and `betterAddr` scoring, which we ported (DECISIONS F-entries).

## Plan

1. **Now (small, independent):** offer upstream issues/PRs for the two F17 bugs and global-replay waste,
   with our regression tests and harness numbers. Each needs to be re-cut against the official crate
   layout (`crates/net/overlay/ts_netstack_smoltcp`), not GeiserX's.
2. **Later (the scalable solution):** an issue or discussion describing exit-node / subnet-router scaling.
   Cover the harness (synthetic WG peers, redirect sink), the §4/§5 results, the sharding stage-1 findings,
   and the design (codex-10/12), citing the Go PRs above. Wait until sharding or stage 2 shows a real
   multi-core gain and we have a remote-generator measurement.
3. Posting is outward-facing: drafts go to the user for approval first.
