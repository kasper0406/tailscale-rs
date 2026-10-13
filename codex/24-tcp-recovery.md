# Brief 24: make the forwarder's smoltcp TCP sender survive burst loss at a bottleneck

## Context

The exit node (`examples/exit_node`, branch `ts-exit/main`, repo `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`)
terminates every forwarded client TCP flow in a userspace smoltcp netstack (vendored at `vendor/smoltcp`, smoltcp
0.13.1 plus one backported RTO fix). For client downloads, smoltcp is the **sender** towards the client's real
kernel TCP (macOS/iOS/Linux, all SACK-capable).

A real-world test from a laptop on Wi-Fi (~25 ms RTT to the node, ~150–180 Mbit/s access link) found
(`docs/DECISIONS.md` F31):
- default 256 KiB window: single download 9 MB/s (window/RTT capped), multi-flow ~half the link;
- 2 MiB window: single download **stalls** (0.3 MB/s, node idle), loaded HTTP latency 15–22 s;
- enabling smoltcp's `socket-tcp-cubic` did not help.

Reproduced offline on branch **`ts-exit/tcp-recovery`** (worktree `/Volumes/git/ts-exit/scratch/wt-lossbench`,
commit `ac68a55` adds `BW_MBPS`/`QUEUE_KB` bottleneck emulation to `ts_forwarder/benches/forward_throughput.rs`):

```
cargo +1.95.0 bench --locked -p geiserx_ts_forwarder --bench forward_throughput --no-run
RTT_MS=25 BW_MBPS=150 QUEUE_KB=<q> TCP_BUF_KB=<buf> <bench-binary> 15 1 down
```
| queue | 256 KiB window | 2 MiB window |
|---|---|---|
| 64 KiB | 0.9 Mbit/s, 7.0 s stalled | 0.2 Mbit/s, 7.8 s stalled |
| 256 KiB | 73 Mbit/s | 0.8 Mbit/s, 7.7 s stalled |

Cubic gives the same numbers. Random `LOSS_PCT` up to 2% does **not** stall, so it is burst loss (a whole window
dumped into a shallow queue) that breaks it.

Likely mechanism, from reading `vendor/smoltcp/src/socket/tcp.rs` (confirm it, don't assume it): the sender
ignores the peer's SACK blocks; it does one fast retransmit on the 3rd dup ACK and then go-back-N from `snd_una`,
re-flooding the queue; every further hole in the same window waits for an RTO; `RTTE_MIN_RTO` is 1000 ms and
doubles to 60 s; no NewReno partial-ACK handling; possibly cwnd isn't enforced or reset sensibly after an RTO.
Note the bench's *receiver* is also smoltcp, with `ASSEMBLER_MAX_SEGMENT_COUNT = 4` (it drops out-of-order
segments beyond 4 holes), which real kernel receivers don't. Make sure the bench isn't receiver-limited (for
example, enable a larger `assembler-max-segment-count-*` feature for the bench peer only) before judging a
sender fix. Also consider whether the forwarder's own receive side (the upload direction) needs more holes.

## Goal

Make the smoltcp **sender** robust to burst loss so downloads through the node get close to the bottleneck rate
with no multi-second stalls, for 256 KiB and 2 MiB windows, queues of 64–256 KiB, and 1 and 8 flows. Also
check the 2 MiB window doesn't hurt loaded latency much more than 256 KiB once loss recovery works (a
congestion controller may be needed for that, so evaluate Cubic or Reno again on top of the fix).

Candidate fixes, roughly in order of value and size (your call after measuring):
1. NewReno fast recovery (RFC 6582): retransmit the next hole on a partial ACK instead of waiting for an RTO;
   no go-back-N re-flood.
2. SACK-based loss recovery on the sender (RFC 6675 style scoreboard): smoltcp already negotiates SACK and
   parses `sack_ranges`; use them to retransmit only the holes.
3. Correct cwnd behaviour on loss and RTO (slow-start restart from a small window rather than resending the
   whole window), and enabling a congestion controller by default.
4. A lower minimum RTO (Linux uses 200 ms; RFC 6298's 1 s is conservative), if measurements justify it.
5. Pacing, only if the above isn't enough.

## Constraints

- Work only on branch `ts-exit/tcp-recovery` in `/Volumes/git/ts-exit/scratch/wt-lossbench`. Never touch the
  live node, `ts-exit/main` or remotes, and don't push.
- Changes to `vendor/smoltcp` should be small, separate commits that could go upstream to smoltcp, each with
  smoltcp-style unit tests (`cargo test` inside `vendor/smoltcp` with the features it needs). Keep the
  existing smoltcp tests passing.
- No regression elsewhere: forward_throughput with no loss (`RTT_MS=0` and `RTT_MS=40 TCP_BUF_KB=4096`, 1 and 8
  flows, both directions) must stay within noise of the base commit; run `cargo test -p geiserx_ts_forwarder`
  and the netstack crates' tests; clippy and fmt clean.
- Commits: `Signed-off-by: Kasper Nielsen <kasper0406@gmail.com>` and `Co-Authored-By: GPT-6.1 (codex)
  <noreply@openai.com>`. No public IP addresses anywhere.
- CPU benchmarks are fine in your sandbox; use `nice` and at most 4 parallel build jobs, because the live exit
  node runs on this machine.

## Deliverable

`/Volumes/git/ts-exit/docs/TCP-RECOVERY-24.md`: confirmed mechanism (with trace evidence), what you changed
and why, a before/after matrix (queue 64/128/256 KiB × window 256 KiB/2 MiB × flows 1/8, down; plus up at
2 MiB; plus the no-loss regression rows), and what you'd recommend as the default `tcp_buffer_size` once it's
fixed. Keep it concise.
