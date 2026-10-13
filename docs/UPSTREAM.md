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

## Filed (2026-10-13)

CONTRIBUTING says the maintainers aren't taking external PRs without contact first, so we filed issues with
deterministic repros and verified patches. Each was checked against `main` b44db0d: the repro fails before
the patch, and the patch passes `ts_netstack_smoltcp` + `ts_runtime` + `tailscale` tests.
- tailscale/tailscale-rs#446: socket `Close` on drop is silently lost when the command channel is full
  (default bound 32; no reaper upstream, so it's a permanent leak and no FIN).
- tailscale/tailscale-rs#447: a cancelled `recv` leaves a blocked command that consumes the next datagram.
- (Withdrawn) a comment on #439 about `ts_derp/src/dial.rs` unwrapping `JoinError::Cancelled`: its evidence
  came from a GeiserX build that included our experimental DERP-dial change, and on official `main` a runtime
  shutdown drops the waiting parent rather than polling it, so it wasn't a valid report there. Deleted.
Not filed: global replay cost (a performance improvement, not a bug; better as part of a later write-up);
listener orphan (already fixed upstream).

## PRs to GeiserX (2026-10-13)

`kasper0406/tailscale-rs` is a fork of the official repo, so PRs to GeiserX (a standalone repo) come from a
second fork, `kasper0406/geiserx-tailscale-rs`.
- GeiserX/tailscale-rs#597: exit-node support + fixes (`ts-exit/exit-node` as branch `exit-node`).
- GeiserX/tailscale-rs#598: data-path performance (`ts-exit/perf` as `exit-node-perf`), stacked on #597.
  Benchmarks: #597 release build vs this PR's production setup on the Mac harness (BENCHMARKS, `pr-*` results):
  1-flow down 0.74 → 3.06 Gbit/s at 24.2 → 1.87 CPU-s/GB; 2 × 8 down 0.65 → 7.51; the base stalled one 2 × 64
  upload flow.
- Bug-fix PRs to GeiserX (each cut from GeiserX `main`, built and tested alone; also contained in #597):
  #599 netstack (lost FIN/Close, abandoned receive, reset listener orphan, blocking try_recv), #600 smoltcp RTO
  backport, #601 peer lifecycle and ordered security updates, #602 netmap state (self-only, Hostinfo, DNS view),
  #603 exit destination guard, #604 betterAddr.
- Commit messages on all `ts-exit/*` layers were rewritten (codex-22, trees verified identical) after the reorg
  had left placeholder bodies. #597/#598 were updated with the same trees.

## Porting onto official tailscale-rs (assessed 2026-10-13)

GeiserX is official `29d87ee` (2026-05-27) plus ~136k lines. Our layers are 1.6k (fixes) + 2.0k (exit-node) +
5.4k (perf). They depend on GeiserX code that official lacks: direct UDP connections (~22k incl. dataplane/runtime;
official #151 is in progress), the forwarder (~3.9k), peerAPI/exit DNS (~2.9k) and route advertisement (#444).
Official maintainers declined community features on #291. Decision: don't port now; port the exit node (~5–8k
lines on official) once #151 lands. A weekly routine (trig_012KpXgYh9fkMu5QTsYbd5m8) watches for that and for a
policy change.
