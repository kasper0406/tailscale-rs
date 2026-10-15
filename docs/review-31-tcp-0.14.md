# Review 31: patched smoltcp 0.14.0 (`ts-exit/tcp-0.14`)

Range: `0e10bc3..cb547f7`. Focus: the delta against the pristine import `038876f`, covering
`vendor/smoltcp/src/socket/tcp{.rs,/recovery.rs,/pacing.rs,/congestion/*}` and the netstack policy
commit `9da013c`. The reviewed worktree was not modified. My scratch copy and harness are in
`scratch/review31-copy/`. Toolchain: `cargo +1.95.0`, at most 4 jobs, `nice -n 10`.

## Verdict

**Safe to run live, as far as the reviewed TCP delta goes. Confidence: high** for panics, hangs
and busy-polls; medium-high for liveness under hostile peers.

- No peer-reachable panic, infinite loop, busy-poll or wedge found, by reading or by fuzzing.
- The review-26 CRITICAL-1 fix is fully present and was not weakened by the Pipe rework.
- B1 and B2 do not create a retry storm, and a timer stays armed whenever data is queued.
- The pacer cannot be driven into a stall or a busy loop.
- What remains is LOW: a hostile peer can slow its own flow or make each poll of its socket more
  expensive. Some INFO notes follow.

The import is verified pristine. I extracted `~/.cargo/registry/cache/.../smoltcp-0.14.0.crate`
(sha256 `b6f8b28a…980f`, which matches PATCHES.md) and diffed it against `038876f:vendor/smoltcp`.
The only differences are `.cargo_vcs_info.json`, `Cargo.lock` and the added `PATCHES.md`.

## Findings

### LOW-1: peer can build unbounded pacing debt on its own flow (confirmed)

- **Where:** `pacing.rs:51-57` (`sent`) is reached from `tcp.rs:3112-3117` for every payload
  segment. That includes fast repairs (`tcp.rs:2915-2925`), which skip the allowance check at
  `tcp.rs:2799-2801`.
- **Trigger:** each partial ACK at or above `recovery_cursor` re-arms a fast repair (`tcp.rs:2337`).
  Each repair adds `len·srtt/(1.2·cwnd)` to `pacer.next` without checking the budget.
  - A peer can send partial ACKs back to back, and it can also inflate `srtt` by trickling ACKs.
    That pushes `next` far beyond one RTT.
  - Fuzz maximum: 8.3 s of debt (release), 30 s (debug run). Benign NewReno sends about one repair
    per RTT, so the debt stays near 1–2 SRTT.
- **Impact:** only that socket's fresh sends are delayed. `poll_at` returns `Time(next)`, so there is
  no spin and no effect on other flows. The 120 s idle timeout still applies.
- **Fix (optional):** clamp the debt in `sent()`, e.g.
  `next = min(next, now + srtt)`, or cap `ready_at` at `now + rto`.

### LOW-2: CPU cost of a full scoreboard in SACK recovery (confirmed, measured)

- **Where:** `recovery.rs:76-148`. `pipe()` runs up to 33 × `hole()`, and each `hole()` scans
  32 ranges. It is called from `sack_retransmit` (`tcp.rs:2517`) and from `cwnd_remaining`
  (`tcp.rs:1516`). Those run on every `poll_at` and `dispatch` while `recovery_end` is set.
- **Measured** (`review31_full_board_cost`, release, M-series):
  - `poll_at` takes **1.9 µs** with 32 ranges in recovery, against **7 ns** otherwise.
  - `process()` of a 3-block ACK against a full board takes about 100 ns.
- **Impact:** the cost is bounded: O(32²) per call, no allocation, about 264 B per socket. 1,000
  hostile downloads held in recovery would add about 2 ms to each interface `poll_at` sweep. That
  is CPU amplification, not a wedge.
- **Fix (optional):** compute Pipe once per dispatch, or keep the ranges sorted so that
  `hole`/`pipe` become linear.

### INFO

- **Codex's 400k-round run covered much less than the report implies (confirmed by instrumenting
  `review30.rs`).** My extended harness covers all of the following.
  - ISS was fixed at 10001, so there was no 2³¹ or 2³² sequence wrap.
  - The emit closure always returned `Ok`, so the B1 failed-emit path never ran.
  - Pacing was active in only **0.42 %** of polls. Peer ACKs arrived at the same instant as the
    send, so `min_rtt` dropped to 5 ms or less and pacing switched off.
  - The drain dispatched every 10 ms whatever `poll_at` returned. It would have missed a stall
    where `poll_at` never schedules a send.
  - The spin detector counted any emission as progress, so a storm of empty ACKs would have gone
    unseen.
  - There was no window scaling, and buffers were 64 KiB only.
- **Spurious RTO trade-off (by design):** with a 200 ms computed floor (`tcp.rs:239`), a peer whose
  delayed-ACK timer is 200 ms (Windows) can occasionally trigger a spurious RTO. smoltcp then
  rewinds and resends the whole flight (`tcp.rs:2741-2750`), with no F-RTO or undo. This affects
  throughput only.
- **One sample of 5 ms or less disables pacing for the socket's lifetime** (`min_rtt` never resets;
  `tcp.rs:224`, `tcp.rs:2468-2474`). This is by design, and a peer can only turn it off for itself.
- **A hostile "cumulative ACK inside a SACK block" state** (the range is trimmed to `left == ack`,
  `recovery.rs:23-26`) can leave `pending_fast_retransmit` set while `fast_repair()` is `None`.
  That suppresses SACK retransmits until the RTO, which is armed at `tcp.rs:2770-2773`. The RTO
  clears the scoreboard. At worst there is one redundant retransmission. No wedge.

## What was checked (all confirmed unless stated)

### 1. Panics

Every new `TcpSeqNumber` subtraction is guarded:

- `hole` returns `cursor < right`.
- `is_lost` relies on `right > max(left, seq)`.
- `pipe` has `repaired > left` and `right > left`.
- `fast_repair` and `sack_retransmit` check `seq < win_end`.
- The dispatch offsets have `seq ≥ una`, because `hole` only moves forward from `una`.

Adds are bounded by `remote_win_len ≤ 65535<<14`, since the scale is clamped to 14 in
`wire/tcp.rs:955`. Pacer arithmetic saturates, and every divisor is at least 1
(`rtt.max(1)`, `rate_window ≥ 1`). `burst − mss ≥ mss`. The Cubic float→int casts saturate,
`cwnd ≥ mss ≥ 48` (`MIN_REMOTE_MSS`), and `cube_root` is unchanged apart from the
`mul_add`/`powi` rewrite. There is no new `unwrap`, indexing or assert on peer data.

### 2. Sequence numbers and SACK

The review-26 validation is intact (`recovery.rs:33-43`): blocks are accepted only when
`0 < left−ack < right−ack ≤ sent−ack`, with an early return when `sent−ack > i32::MAX`. Walks are
bounded by `len+1` steps and stop when there is no progress.

The harness checked, after every step, that every stored range lies in `(una, snd_nxt]`. That held
across 750k wrap rounds. An ACK for unsent data (`ack > snd_nxt`, which smoltcp accepts up to
`una + tx_len`) takes the early return, and the stale ranges were already trimmed. Only an RTO
rewinds `snd_nxt`, and the RTO clears the board. Overlapping, reversed, DSACK, half-space and
reneging blocks were all exercised.

### 3. B1 and B2

- **B1:** a failed repair keeps `pending_fast_retransmit` and arms `Retransmit(rto)`
  (`tcp.rs:2770-2773`). In production, `WakingPipeDev::transmit` returns `None` only on disconnect.
  Dispatch errors (no route) are silenced for 1 s by `SocketMeta::neighbor_missing`
  (medium-ip `has_neighbor` = route exists). So there is no retry storm. Retrying on every poll
  while the device is full is the same as stock behaviour for any queued data.
- **Zero-window paths:**
  - When the probe timer stops and data is in flight, the RTO or the recovery repair is re-armed
    (`tcp.rs:2363-2373`).
  - An RTO on a closed window arms the probe timer (`tcp.rs:2774-2778`).
  - The harness never saw an ESTABLISHED socket with queued data and `poll_at == Ingress` (0 of
    13.6 G polls).

### 4. Pacing and `poll_at`

`ready_at(now) ≤ now` implies `allowance(now) ≥ mss`. Algebraically,
`floor(credit·rw/(rtt·1000)) ≤ burst−mss`. So `poll_at` returning `Now` always leads to a send.
Otherwise `poll_at` returns `Time(..)`, never `Ingress`. Production sleeps with
`tokio::time::sleep`, which rounds up, so there is no early-wake spin.

Fuzz result: the worst run of `Now` without an emission was 1 consecutive poll. There were 0
empty-segment storms, and in 0 cases did dispatch send data after `poll_at` said `Ingress`.

### 5. Bounds

- The scoreboard is a heapless `Vec<_, 32>`. An overflowing push is dropped, and only after a merge
  has freed a slot.
- `update` is O(3·32²) per ACK.
- Each socket gains about 300 B, with no heap allocation.

### 6. Interop and data retirement

Every SACK path is gated on `sender_sack_enabled && remote_has_sack`. Non-SACK peers use
NewReno partial-ACK recovery, and 20 % of harness rounds drained correctly without SACK. Transmit
data is freed only by `dequeue_allocated(ack_len)` on a cumulative ACK (`tcp.rs:2207`).

### Harness (`scratch/review31-copy/vendor/smoltcp/src/socket/tcp/review31.rs`)

The harness is the review-26/30 generator plus:

- ISS near `0xFFFF_xxxx`, `0x7FFF_xxxx` or random.
- 64 KiB and 1 MiB buffers, window scale 0–4, peer MSS 48–65535.
- 0/5/30 % emit failures.
- RTT-like time steps so that pacing engages.
- Partial ACKs and SACK blocks inside the flight.
- A production-faithful drain that dispatches only when `poll_at` is due and ACKs after 20 ms.

**Release (overflow-checks off): 4 × 250k rounds.** That is 13.6 G polls, 1.85 G segments,
897 M peer ACKs and 750k wrap rounds. It covered:

- 129.5 M failed emits, 1.6 M of them failed repairs.
- 1.58 G pacing-blocked polls.
- 29 M SACK retransmits, 4.3 M fast repairs and 27.8 M partial ACKs in recovery.
- 100 M zero-window ACKs.

Result: **0 panics, 0 wedges, 0 drain stalls, 0 unarmed sockets, 0 empty storms.**

**Debug (overflow-checks and debug-asserts on): 4 × 25k rounds.** 1.36 G polls, with the same
zeros. Slow drains of up to 38 s are a harness artifact: the hostile phase leaves the window at 0,
the probe/RTO timers backed off and `srtt` inflated, and the drain peer stays silent until we send.
They are bounded by the 60 s maximum backoff.

The author lib suite in the same configuration: 464 passed.

## Not checked

- Live behaviour against real Linux, Windows or macOS peers, including the spurious-RTO rate with
  the 200 ms floor.
- The netstack's custom listener and replay paths, which are outside this diff.
- 32-bit targets and the `segmentation-offload`, `defmt` and `log` feature builds. The harness ran
  with `std` and without `log`; production uses `alloc`/`async`.
- A formal proof of the stock smoltcp invariant "`seq_to_transmit` implies dispatch emits", which I
  relied on together with the fuzzing.
