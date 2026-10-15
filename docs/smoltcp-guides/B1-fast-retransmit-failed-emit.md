# B1: a fast retransmit is lost if emitting it fails

**Kind:** bug. **Effort:** 0.5–1 day. **Draft report:** `docs/smoltcp-drafts/B1-fast-retransmit-lost-on-failed-emit.md`.

## What happens

1. Three duplicate ACKs arrive. `process` sets `pending_fast_retransmit = true` and moves the timer to
   `FastRetransmit` (`src/socket/tcp.rs` ~2566–2585).
2. `dispatch` picks the oldest unacknowledged segment for the repair and **clears the flag** (~2712) before it
   calls `emit(...)?` (~2864).
3. If `emit` returns an error (device out of TX buffers, neighbour not resolved, etc.), the `?` returns early.
   The flag is already false, so the next `dispatch` doesn't retry the repair. The early return also skips the
   bookkeeping that would arm the retransmit timer, which `process` had set to idle.
4. Result: lost segment, no repair, no timer (`Idle { keep_alive_at: None }`). Without new data, a keepalive or
   further peer traffic, the connection is stuck.

dispatch's own documentation says a failed emit should be retried, so this violates the intended semantics.

## Fix idea

- Clear `pending_fast_retransmit` only after a **successful** emit of a data segment (in our version:
  `if self.pending_fast_retransmit && repr.segment_len() > 0 { self.pending_fast_retransmit = false; }` after
  the emit).
- While a repair is pending and data is in flight, keep a retransmit deadline armed instead of going idle, so a
  repair that keeps failing still falls back to the RTO.
- Don't call the congestion controller's `on_loss` again on the retry (cwnd must be cut once per loss event).
- Don't try the repair into a zero window (`remote_win_len == 0`); let the persist/probe logic handle that.

## Tests

- The repro (fails on main): set up 4 segments, 3 dup ACKs, make the first `dispatch` callback return `Err(())`,
  then a successful `dispatch` must emit exactly one repair of the first segment, with the sent frontier
  (`remote_last_seq`) unchanged and cwnd reduced only once.
- Repeated failures: timer still armed; after the RTO, ordinary retransmission takes over.
- Zero window while a repair is pending: no repair attempt, persist timer instead.

## Our reference

Commit `050cb5c` on `ts-exit/tcp-0.14` (`smoltcp: retain fast repair after failed device emission (B1)`), test
`brief29_bug_fast_retransmit_survives_failed_emit`. Review 31 checked that it doesn't cause a retry storm: in
practice emit only fails briefly, and neighbour-resolution failures are rate-limited by the interface.
