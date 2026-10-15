# Feature: NewReno partial-ACK recovery (RFC 6582)

**Kind:** feature (an improvement to classic Reno, not a fix of an existing NewReno). **Effort:** 2–3 days,
mostly tests. **Matters for us:** yes; together with SACK and pacing it's what makes larger windows safe.

## The problem

smoltcp main implements classic Reno fast retransmit:

- On 3 duplicate ACKs it repairs the oldest segment once (go-back-N was removed upstream in #1155, `d84a898`).
- The next ACK that acknowledges new data ends recovery: the dup-ACK count resets, Reno/Cubic leave recovery,
  and the ordinary RTO restarts.
- If the flight had **two or more** holes, that ACK is only a *partial* ACK. It covers the repaired segment but
  stops at the next hole. Classic Reno has no rule for that, so the second hole waits for a full RTO (≥1 s on
  0.14). With burst losses this happens all the time.

The repro test `brief29_feature_newreno_partial_ack_repairs_next_hole` (in codex's scratch clone,
`scratch/smoltcp-upstream-29-repros.patch`) shows main sends nothing after a partial ACK.

## What NewReno adds

- When entering fast recovery, remember `recover` = the highest sequence sent so far (`snd_nxt`). In our code that
  is `recovery_end = Some(self.remote_last_seq)`.
- **Partial ACK** (it acknowledges new data but `ack < recover`): stay in recovery, immediately retransmit the
  next unacknowledged segment, and *partially deflate* cwnd by the amount acknowledged. Don't cut ssthresh again
  (one cwnd reduction per loss episode).
- **Full ACK** (`ack >= recover`): exit recovery normally.
- **RTO** during recovery: clear `recover` and fall back to ordinary RTO behaviour.
- New data may still be sent within cwnd during recovery; `recover` must stay at the original boundary, not
  follow `snd_nxt` forward.
- Don't enter a new recovery on 3 dup ACKs while one is already active (`recovery_end.is_none()` guard).

## Integration points (from our commit `b8087e5`, +124/−11)

- A `recovery_end: Option<TcpSeqNumber>` field on the socket, reset on connection reset and RTO.
- In the ACK path: if `ack_len > 0` and `ack < recovery_end`, call a new controller hook
  `on_partial_ack(now, ack_len)` instead of `on_ack`, and re-arm the fast-retransmit timer so `dispatch` repairs
  the next hole.
- In loss entry: call `on_loss` only when starting a new episode.
- Controllers: implement `on_partial_ack` for **both** Reno and Cubic (partial deflation, no exit from recovery).
  Upstream's controller API changed in June, so this is the part to design against main, not port.

## Tests

- Two losses in one flight; three losses; losses at the start, middle and end of the flight.
- A partial ACK followed by the full ACK; an RTO during recovery.
- Fresh sends during recovery don't move `recover`.
- Sequence-number wraparound across `recover` (start near 2^31 and 2^32).
- Failed emit during recovery (interaction with B1); a zero window during recovery; FIN in flight.
- Peers without SACK (NewReno is precisely the non-SACK recovery).
- For measurements, adapt smoltcp's own `tests/netsim.rs` (bandwidth plus tail-drop queue) rather than our
  forwarder bench; report goodput and longest pause before and after.

## Upstream context

#558 (NewReno) closed unmerged but the maintainer (Dirbaio) was positive; the June overhaul was split into
#1154–#1157, and he engaged constructively with those. A short issue first is a good idea, since their policy asks
for discussion before features.
