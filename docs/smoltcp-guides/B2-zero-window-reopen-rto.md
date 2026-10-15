# B2: the retransmit timer is lost when a zero window reopens with data in flight

**Kind:** bug. **Effort:** 1–2 days (small fix, careful edge cases). **Draft:** `docs/smoltcp-drafts/B2-zero-window-reopen-drops-rto.md`.

## Background

A receiver advertises how many bytes it can take (the window). With window 0 the sender may only send small
probes ("persist"/zero-window probes) on their own timer until the window opens again. RFC 6298 §5 separately
requires a running retransmission timer whenever data is outstanding (sent but unacknowledged).

## What happens on main

`src/socket/tcp.rs` ~2265–2277:

1. 12 bytes are in flight as two segments. The peer ACKs the first 6 bytes and advertises window 0 in the same
   ACK. The code replaces the running retransmit timer with the zero-window-probe timer. Allowing that after a
   *partial* ACK is the first problem: 6 bytes are still outstanding.
2. The peer reopens the window without acknowledging the remaining 6 bytes (for example the second segment was
   lost). The reopen branch calls `set_for_idle` unconditionally.
3. Now 6 bytes are in flight and no timer runs. Nothing is retransmitted until the peer sends something else; in
   the repro, nothing happens for a simulated hour.

The same transition can happen with real peers whose receive buffers fill up (slow application readers), which is
common for downloads to phones and laptops.

## Fix idea

- On window reopen: if `flight_size() > 0`, arm the retransmit timer (`set_for_retransmit(now, rto)`) instead
  of going idle; only go idle when nothing is outstanding.
- Optionally (cleaner): don't swap the retransmit timer for the probe timer while data is in flight.
- Related edge case we also fixed (commit `5952e3a`): when an RTO expires while the window is closed and queued
  data remains, keep the persist timer active rather than idling.

## Tests

- The repro (fails on main): close the window with a partial ACK, reopen it without ACKing the rest, assert the
  timer is a retransmit timer and that the remaining segment is retransmitted after one RTO.
- Lost window-update: the reopening ACK is lost; probes must continue with backoff.
- Queued but unsent data with nothing in flight and window 0: persist probes, not RTO.
- Probe backoff and the congestion-window exemption for probes stay as they are.

## Our reference

Commits `22e892c` (B2) and `5952e3a` (persist after RTO on a closed window); tests
`brief29_bug_reopened_window_retains_retransmit_timer`, `brief30_queued_data_after_zero_window_arms_persist`.
This was first found by our review 26 harness (zero-window-then-reopen flows stalled until keepalive).
