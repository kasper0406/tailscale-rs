# Review 26 — TCP sender recovery (smoltcp 0.13.1 fork + netstack wrappers)

Range reviewed: `git diff ac68a55..3ce8908 -- vendor/smoltcp ts_netstack_smoltcp ts_netstack_smoltcp_core`
Branch: `ts-exit/tcp-recovery`. Toolchain: `cargo +1.95.0`. Read-only on code; scratch copy at
`/Volumes/git/ts-exit/scratch/review26-copy` (not the reviewed worktree).

## Verdict

**Not safe to run live as-is.** Confidence: high.

There is **one confirmed critical, remotely-triggerable denial of service**: a malicious (or
buggy) peer that negotiated SACK can drive the new sender SACK scoreboard into a state where
`Scoreboard::pipe()` **loops forever**. Because `pipe()` is called from both `dispatch()` and
`poll_at()`, the netstack polling task spins at 100% CPU and never returns — the exit node stops
forwarding all flows. This is reached through the real `Socket::process()` path from ordinary ACK
segments. One fix (bounding SACK range width to the real flight) closes it; the rest of the change
is otherwise sound and the authors' 685 smoltcp tests pass.

Everything else I checked is acceptable: cumulative-ACK-only data retirement holds, the scoreboard
is bounded at 32 ranges, the 200 ms floor applies only to the computed RTO, non-SACK interop is
preserved, and the assembler 4→32 change is cheap and bounded.

---

## Findings

### CRITICAL-1 — `Scoreboard::pipe()` infinite loop from peer SACK input (confirmed)

- **Where:** `vendor/smoltcp/src/socket/tcp/recovery.rs:113-123` (`pipe`), enabled by missing
  width validation in `recovery.rs:29-49` (`update`). Hang is reached from
  `vendor/smoltcp/src/socket/tcp.rs:2434` (`seq_to_transmit` → `congestion_allowance`,
  `tcp.rs:260-275`) and `tcp.rs:2941` (`poll_at`), i.e. every `dispatch`/`poll_at` while a
  recovery episode is open with `remote_has_sack`.
- **Mechanism (confirmed):** `update()` validates each SACK block only with
  `left <= ack || right <= left || right > sent` — all **wrapping** `TcpSeqNumber` comparisons. It
  never bounds a block's *width*, and the merge step (`left.min(start)`, `right.max(end)`, also
  wrapping) can synthesize a stored range spanning ~2^31 of sequence space. With two such ranges,
  `pipe()`'s loop `next = right` where `(left,right) = hole(next,end)` can produce a `right` that
  is *behind* `next` under wrapping order, so `next` cycles and the `cursor >= end` termination in
  `hole` (`recovery.rs:76-84`) never fires.
- **Confirmed repro (captured from a randomized socket-level harness driving real
  `Socket::process`):**
  ```
  ack=192401  cursor=186666  end=203569
  ranges=[(192401, -2147322346), (925863071, -1221621680)]
  pipe(ack=192401, cursor=186666, end=203569, mss=1200)  // never returns
  // cycle: next=192401 -> cursor crosses both wide ranges -> right folds back to 192401 -> next=192401 ...
  ```
  A stack sample of the hung process showed 100% of time in
  `review26 -> poll_at -> seq_to_transmit -> Scoreboard::pipe` (recovery.rs:114-116), i.e. a single
  call that does not return. Reachability through `Socket::process()` is therefore established
  (the harness only feeds peer-plausible ACK/SACK segments through the real socket API).
- **Trigger in production:** the peer negotiates SACK (it controls the SYN `sack_permitted`), then
  ACKs our data with crafted SACK blocks. The scoreboard is the *sender* scoreboard, so any peer we
  are sending bulk data to — e.g. a tailnet client downloading through the exit node — can trigger
  it. A panic kills the node; this is worse: a silent, unkillable spin that wedges the whole
  netstack task.
- **Suggested fix:** in `update()`, reject or clamp any block that does not lie within the current
  flight measured as unsigned offsets from `ack`. Concretely, accept a block only when
  `0 < (left − ack) < (right − ack) <= (sent − ack)` using the wrapping *difference* (the same
  `TcpSeqNumber - TcpSeqNumber` the socket already relies on, which is bounded because the real
  flight `sent − ack` is < buffer size ≪ 2^31). That keeps every stored range narrower than the
  real flight, which makes `hole()`/`pipe()` strictly monotone and guarantees termination. As
  defense in depth, also cap `pipe()`'s loop at `ranges.len()+1` iterations. Add a regression test
  with a ~2^31-wide block (and the captured two-range state) asserting `pipe()` returns.

### MEDIUM-1 — production build runs with overflow-checks OFF (confirmed; context + hardening)

- **Where:** root `Cargo.toml:215-228`. `[profile.exitnode]` inherits `[profile.release]`; neither
  sets `overflow-checks`, and there is no `.cargo/config.toml` override (only `build.jobs` and
  `RUST_TEST_THREADS`). So Cargo's release default applies: **overflow-checks = false**.
- **Impact:** plain `usize`/`u32`/`u64`/`i64` overflow in the new pacing/congestion/recovery
  arithmetic wraps silently in production rather than panicking. The **explicit** `panic!`s in
  `TcpSeqNumber`'s `Add`/`Sub` (`vendor/smoltcp/src/wire/tcp.rs:44,55,73`) still fire in release and
  remain a node-killing surface; I fuzzed the new scoreboard and socket paths (hundreds of
  thousands of randomized adversarial ACK/SACK rounds) and found **no** reachable `TcpSeqNumber`
  underflow panic — but see "could not fully check" below, as the `pipe()` hang masks exploration
  past the point it fires.
- **Suggested action:** no code change required for correctness, but consider
  `overflow-checks = true` on `[profile.exitnode]` only if you would rather a wrap become a crash —
  given a panic already kills the node, this is a double-edged choice; the real remedy is bounding
  inputs (CRITICAL-1) so neither wrap nor panic is reachable. Document the decision either way.

### LOW-1 — pacing `Instant`/`Duration` arithmetic relies on release wrap, not bounds (suspected, low risk)

- **Where:** `vendor/smoltcp/src/socket/tcp/pacing.rs:46-49` (`ready_at`, `next − Duration`) and
  `pacing.rs:52-55` (`sent`, `next + Duration`).
- **Note:** these are `Instant` (i64 µs) ± `Duration`. Credits/delays are bounded in practice
  (≈ RTT·len/window, ≤ tens of seconds), so no overflow in realistic operation, and the added unit
  test guards the 32-bit credit-narrowing case. With overflow-checks off they would wrap rather than
  panic; with checks on, a pathological `srtt`/window could in principle panic. Low risk, flagged
  for completeness. No change required.

---

## What I checked and the result

- **Panics from peer input (reachable):** fuzzed `Scoreboard` directly (400k randomized rounds of
  arbitrary/reversed/wrapping/out-of-window SACK blocks with moving ack/sent) and the full socket
  via `Socket::process` (randomized MSS 1–65535, SACK on/off, Nagle on/off, zero windows, random
  cumulative ACK + SACK). Only CRITICAL-1 surfaced. No `unwrap`/`expect`/indexing panic, no
  divide-by-zero (pacing division by `srtt`/window is gated behind `pacing_window()` which requires
  `srtt > 5`, `min_rtt > 5`, and a window ≥ MSS > 0), no `debug_assert` firing in the exercised
  paths. Float→int casts in Cubic are Rust's saturating casts (no UB/panic).
- **Scoreboard bound holds:** `heapless::Vec<_,32>`, over-full `push` is dropped (`let _ =`), so it
  never exceeds 32; confirmed by the authors' `full_scoreboard_forgets_safely` test and by my
  fuzzer asserting `len() <= 32` every step. Forgetting a range only causes redundant retransmit,
  never premature data retirement.
- **Cumulative-ACK-only retirement:** tx data is freed solely by `dequeue_allocated(ack_len)` on
  cumulative ACK (`tcp.rs:2085`); SACK only steers retransmission. Confirmed by
  `test_sack_recovery_skips_received_ranges` ("SACK must not free transmit data").
- **ACK acceptability unchanged:** `process()` still rejects ACKs below `snd_una` and challenges
  ACKs above `snd_nxt` (`tcp.rs:1715-1733`); the new recovery logic runs after that gate.
- **RTO / backoff:** 200 ms floor is applied only to the *computed* RTO in `sample()` via
  `.clamp(self.min_rto, RTTE_MAX_RTO)`; initial 1 s handshake RTO and exponential backoff are
  preserved (authors' `test_configured_rto_floor_survives_reset_and_preserves_backoff`, and the
  floor survives `reset()`). `min_rto` is clamped to `[1, RTTE_MAX_RTO]`.
- **Non-SACK interop:** every scoreboard/sack path is gated on `self.remote_has_sack`; a peer that
  does not offer SACK falls back to NewReno partial-ACK recovery. (Note: this gate does not mitigate
  CRITICAL-1, since the peer itself negotiates SACK.)
- **Reno/Cubic fixes:** ACK-clocked growth, `len==0` early return (no growth on pure dup-ACKs),
  timeout restart to `min_cwnd`, segment-unit Cubic curve, immediate multiplicative decrease — unit
  tests added and passing; threshold arithmetic has no division by zero (constant divisors).
- **Assembler holes 4→32:** `contigs: [Contig; 32]`, `Contig` = 2×usize = 16 B, so ~512 B/socket
  (+448 B vs 4 holes) — negligible beside the ~512 KiB/socket TCP buffers even at ~1000 flows.
  `add()` is O(32)/segment (bounded, no O(n²)); overflow returns `TooManyHolesError` and the segment
  is dropped, so no unbounded growth.
- **`run.rs` timestamp change:** recreating the `IoPoller` each poll via
  `poll_fn(|cx| …wait_io_async(clock(), dev)…)` refreshes the ingress timestamp on every wakeup.
  The added `receive_after_pending_uses_current_clock` test shows it stays `Pending` until real data
  arrives (no busy-loop) and uses the current clock. Looks safe.
- **Authors' suite:** `cargo +1.95.0 test --release --lib --features
  socket-tcp-reno,socket-tcp-cubic,assembler-max-segment-count-32` → 685 passed, 0 failed (with my
  review-only test modules skipped).

## What I could not fully check

- **`TcpSeqNumber` panic surface downstream of CRITICAL-1:** once the scoreboard reaches the
  hanging state, the fuzzer cannot proceed, so paths that run *after* a wide-range episode are not
  exhaustively cleared. After the width fix lands, re-run the randomized socket harness to confirm
  no `TcpSeqNumber` subtraction underflow is reachable (I consider it likely clear but not proven).
- **`congestion_allowance` non-SACK subtraction** `self.remote_last_seq - self.local_seq_no`
  (`tcp.rs:269`) relies on the invariant `remote_last_seq >= local_seq_no`, which `process()`
  re-establishes (`tcp.rs:2156-2158`). I reasoned this holds at every `dispatch`/`poll_at` entry but
  did not construct an independent adversarial proof; worth a targeted assertion/test.
- **Live/integration behaviour** (throughput, real loss recovery, interaction with the forwarder's
  per-flow socket churn) was not exercised — this was a code+unit/fuzz review only.
- **32-bit targets:** the exit node is 64-bit; I did not build/run the 32-bit path beyond the
  authors' existing pacing unit test.

---

## Re-check after 4e6b554

Branch `ts-exit/live-candidate`, worktree `scratch/wt-live-cand` (read-only). The vendored smoltcp
differs from the reviewed `3ce8908` only in `recovery.rs`. 44c854d (no Nagle) is outside smoltcp.
Harness copy: `scratch/review26b-copy`.

**Verdict: safe to run live from the TCP-sender standpoint. Confidence: high.** CRITICAL-1 is
fixed. A long randomized run found no panics and no busy-loops. The only stalls it found are a
smoltcp problem that predates this change. The patch makes them less frequent, and production
keepalive/timeout settings bound them (LOW-2).

### The fix itself
- `update()` measures each block as a wrapping offset from `ack` and accepts it only when
  `0 < left−ack < right−ack <= sent−ack`. It returns early if `sent−ack > i32::MAX`. So every
  stored range lies inside `(ack, sent]`. Merges stay inside the flight, and a later `ack`/`sent`
  only moves forward. That keeps the wrapping comparisons in `hole`/`pipe` a consistent order.
  DSACK and below-`ack` blocks get a huge offset and are rejected. `right == sent` is still accepted.
- `hole()` and `pipe()` are now bounded by `ranges.len()+1` steps, and `pipe()` stops if `right`
  does not advance. This is a second safeguard. With the validation in place, a malformed board
  cannot be reached. The new `pipe_terminates_on_a_malformed_board` test shows the captured state
  now terminates.
- The authors' suite passes: 687 tests, including `rejects_blocks_outside_the_flight` and
  `pipe_terminates_on_a_malformed_board`.

### Harness run (socket level, through `Socket::process`/`dispatch`/`poll_at`)
- Randomized per round: Cubic or Reno; pacing on/off; SACK on/off; Nagle on/off; MSS in
  {1, 6, 536, 1200, 1460, 9000, 65535}. Peer input per step: arbitrary or acceptable cumulative
  ACKs, arbitrary/wide/reversed/half-space SACK blocks, windows including 0, and bursts of
  duplicate ACKs with SACK. Each round then has a cooperative-peer drain phase.
- After every `process`/`dispatch` the harness asserts `snd_nxt >= snd_una`. That is the
  precondition of the non-SACK `remote_last_seq - local_seq_no` in `congestion_allowance`.
  Correction to the line reference above: it is `tcp.rs:2347` (function at `tcp.rs:2338-2353`), not
  `tcp.rs:269`, which was a diff-relative line. Any `TcpSeqNumber` Add/Sub panic is caught and recorded.
- **Ran 400,000 rounds (4 seeds × 100k): about 2.15 billion segments emitted and 371 million peer
  ACKs processed. Result: 0 panics (no `TcpSeqNumber` underflow or overflow, the invariant held
  throughout, no other panic). No `poll_at == Now` busy-loop (worst run of consecutive
  no-progress polls was 2).**
- Sanity check on the harness: the same harness against the pre-fix scoreboard hit the `pipe()`
  loop in 538 of 2000 rounds. Against the fix it never did.

### New finding

**LOW-2 — sender stall with no timer running (existing smoltcp behaviour, not a regression;
confirmed).** In 17,398 of 400k harness rounds (4.35%), the socket ended ESTABLISHED with data
queued, timer `Idle`, and no recovery episode, so it never sends again unless the peer sends
something first. 80% had a zero window and nothing in flight: smoltcp has no persist timer in that
state. 20% had an open window with data in flight. That case is reproduced deterministically: a
partial ACK advertising window 0 replaces the RTO timer with the zero-window-probe timer
(`tcp.rs` ZWP start condition `timer.is_idle() || ack_len > 0`). A later reopen stops the probe
timer and sets `Idle` while data is still outstanding. Nothing is retransmitted for a simulated hour.

- **Not a regression.** Baseline ac68a55 behaves the same in the deterministic test. On the same
  seed (1, 2000 rounds) it stalls more often: 105 rounds (73 zero-window, 32 open-window) vs
  86 rounds (74 zero-window, 12 open-window) with the fix. The patch cuts the open-window stalls
  to well under half and leaves the zero-window ones unchanged.
- **Bounded in production.** The harness runs with keepalive off. With the netstack defaults
  (`ts_netstack_smoltcp_core/src/config.rs:104-105`: keepalive 60 s, timeout 120 s), a responsive
  peer's replies to keepalives count as duplicate ACKs, and the stalled data is retransmitted after
  about 180 s. A silent peer's socket is closed at 120 s. Effect: one flow stalls for up to about
  3 minutes after a zero-window episode or a lost window update. It affects only the triggering
  flow, not the node.
- **Fix (upstream-worthy, optional before going live):** do not replace a running retransmit timer
  with the probe timer while data is in flight. When the probe timer stops on reopen with data
  outstanding, re-arm the RTO instead of going `Idle`. Also add a persist timer for the
  zero-window, nothing-in-flight case.
