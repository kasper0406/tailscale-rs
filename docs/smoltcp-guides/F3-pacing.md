# Feature: pacing

**Kind:** feature. **Effort:** 2–4 days after the design is agreed. **Matters for us:** large in our emulated
shallow-queue benchmark (median 51 → 112 Mbit/s on stock 0.14.0, `docs/PACING-AB-32.md`), but one real Wi-Fi A/B
showed no clear gain (F37), and review 34 rates its real-world priority as unproven.

## The problem

Without pacing, a TCP sender sends as much as cwnd allows the moment ACKs arrive, back to back at line rate. A
1 MiB window can mean hundreds of packets in one burst. A home router or Wi-Fi access point with a shallow queue
(64–256 KiB) drops the tail of the burst, so you get burst losses, which recovery then has to clean up. Pacing
spreads the window over the round trip, which reduces queue overflow from our own bursts. It doesn't prevent
losses from other causes (radio loss, cross traffic). Linux paces by default
(`fq`/TCP internal pacing); smoltcp doesn't pace at all.

## Design we used (commit `21e8417`, +269/−5, opt-in via `set_pacing_enabled`, plus the fix `92c7b2e`)

- **Rate:** `1.2 × cwnd / SRTT` (the 1.2 headroom is our tuning, similar to Linux's slow-start/CA pacing ratios).
- **Burst:** allow `max(window/8, 2 MSS)` at once, capped at 32 MSS after a loss.
- **State:** a `Pacer { next: Option<Instant>, congested: bool }`. `sent()` advances `next` by
  `len × srtt / rate_window`. `allowance()` says how many bytes may go now. `ready_at()` gives the deadline that
  `poll_at` returns when the budget is empty.
- **Priorities:** pure ACKs, the first fast repair, RTO retransmits, delayed ACKs and timeouts are never delayed
  by the pacer.
- **Bypass:** no pacing until there's a valid RTT estimate, and none if the baseline RTT is ≤ 5 ms (LAN, where it
  only costs CPU).
- **Bounded arithmetic:** saturating maths everywhere, safe narrowing on 32-bit, and debt capped at one SRTT
  (`92c7b2e`, from review 31: repeated fast repairs plus a peer-inflated RTT could otherwise push one flow's next
  send many seconds ahead).

## Things a reviewer will ask about

- `poll_at` must never return "now" without at least one MSS of budget (busy loop), and never "wait for input"
  when a paced send is due (stall). Review 31 verified this for ours; write tests for both.
- Interaction with Nagle and delayed ACKs; idle restart (don't send a full window burst after idling).
- 16/32-bit targets: `u32` RTT in ms × bytes can overflow; use u64 and saturating maths.
- Fairness and latency, not just goodput: measure loaded RTT (we use a 1-byte echo probe) as well.
- The multiplier and burst size are policy; maintainers may want them configurable.

## Upstream context

#1090 (a BBR port) is open and explicitly says missing pacing blocks it, so there's demand but no agreed design.
#949 (throughput collapse with large buffers) looks like our workload; the maintainer's advice there was device
backpressure, `max_burst_size` and enabling congestion control. We measured `max_burst_size` and it doesn't help
(in 0.14 it clamps the advertised window, not the send burst). Start with a design issue: the API (per-socket
flag vs feature), timer integration and default off.
