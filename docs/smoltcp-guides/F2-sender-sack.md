# Feature: sender-side SACK recovery (RFC 2018 + RFC 6675 subset)

**Kind:** feature. **Effort:** 3–5 days (more if maintainers want a different design). **Coordinate first:** the
author of open PR #1195 (receiver-side SACK improvements) says they have sender recovery implemented privately.

## The problem

With SACK, the receiver tells the sender which byte ranges *after* the hole it already has (up to 3–4 ranges per
ACK). smoltcp already *sends* SACK information as a receiver (since 2018) but ignores incoming SACK blocks as a
sender. Without them, the sender has to guess which segments are missing: NewReno repairs one hole per round trip,
SACK lets it repair all known holes at once.

## Design we used (commit `94847a3`, +525/−30, opt-in via `set_sender_sack_enabled`)

- **Scoreboard:** a fixed-size `heapless::Vec<(TcpSeqNumber, TcpSeqNumber), 32>` of SACKed ranges: 256 bytes per
  socket, no heap. Merge overlapping ranges, and drop ranges at or below the cumulative ACK.
- **Advisory only:** SACKed data is never freed from the send buffer. Only the cumulative ACK frees data (the
  receiver may renege). An RTO clears the scoreboard.
- **Recovery (RFC 6675 subset):** a segment is considered lost when there are 3 discontiguous SACKed ranges or more
  than 2 MSS SACKed above it (`IsLost`). Estimate *Pipe* (bytes really in the network: in flight minus SACKed minus
  lost) and send while `Pipe < cwnd`, repairing the lowest lost hole first and then new data. No "rescue"
  retransmission and no RACK, so call it a subset.
- **Overflow:** if 32 ranges aren't enough, forget the oldest. That only costs redundant retransmission, never
  correctness.
- If both sides negotiated SACK, it replaces NewReno's one-hole-per-RTT repair; without SACK, NewReno applies.

## Security: the part that must be in the first commit

SACK blocks come straight from the peer. Our first version validated each block by comparing endpoints with
wrapping sequence comparisons, and a peer could send blocks whose merge spanned ~2^31 of sequence space. The
wrapping order then became inconsistent and the `pipe()`/`hole()` walks looped forever. That wedged the whole
netstack at 100% CPU, and our review 26 found it with a randomized harness. The fix (our `4e6b554`):

- accept a block only if `0 < left−ack < right−ack <= sent−ack`, using wrapping offsets from the cumulative ACK,
  so every stored range lies inside the current flight (which is < 2^31);
- bound every walk by the number of ranges and require progress.

Include this from day one, with tests that feed hostile blocks (out of window, reversed, overlapping, beyond
`snd_nxt`, below `snd_una`, half-space spans). Also mention fuzzing in the PR. Review 31's harness
(`scratch/review31-copy/.../review31.rs`) is a model for what to cover.

Review 31's LOW-2: with a full 32-range scoreboard, recomputing Pipe on every `poll_at` costs ~1.9 µs. Computing
it once per dispatch, or keeping ranges sorted, would be cleaner.

## Tests

- SACK negotiated vs not; reneging (data SACKed then not cumulatively ACKed until RTO).
- Multiple holes repaired in one round trip; Pipe accounting with fresh data; reordering (not loss).
- Duplicate/overlapping/out-of-window blocks; scoreboard saturation; wraparound near 2^31/2^32.
- RTO clears the scoreboard; only the cumulative ACK frees buffer space.

## Upstream fit

Issues #256 and #1146 ask for SACK. Maintainers care about no_std, no heap, small per-socket cost and feature
gates, so propose it opt-in (a cargo feature or a runtime flag) with a configurable range limit. Ask on
#256/#1195 first.
