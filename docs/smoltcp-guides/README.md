# smoltcp work: guides for writing it yourself

Notes for implementing the smoltcp fixes and features by hand (smoltcp's CONTRIBUTING requires human-authored
code). Line numbers refer to smoltcp main at `87b463f` (2026-10-03), as analysed in
`docs/SMOLTCP-UPSTREAM-29.md`. Our private implementation (reference only, don't copy into PRs) is on
`ts-exit/tcp-0.14` in `repos/geiserx-tailscale-rs`, under `vendor/smoltcp`.

| Guide | Kind | Effort | Matters for our node? |
|---|---|---|---|
| [B1: fast retransmit lost on failed emit](B1-fast-retransmit-failed-emit.md) | bug | 0.5–1 day | yes |
| [B2: zero-window reopen drops the RTO](B2-zero-window-reopen-rto.md) | bug | 1–2 days | yes |
| [B3: Cubic without std](B3-cubic-no-std.md) | bug | ~0.5 day | no (we build with std) |
| [NewReno partial-ACK recovery](F1-newreno.md) | feature | 2–3 days | yes |
| [Sender SACK](F2-sender-sack.md) | feature | 3–5 days | yes (coordinate with #1195 first) |
| [Pacing](F3-pacing.md) | feature | 2–4 days | yes (largest single gain) |

Suggested order: B1, B2, (B3), then open discussions for SACK and pacing while you write NewReno.

**Shared background.** A TCP sender keeps a *flight* of unacknowledged bytes (`snd_una`..`snd_nxt`). When the
peer misses a segment, it keeps acknowledging the byte before the hole (duplicate ACKs). On the third duplicate
ACK the sender does a *fast retransmit* of the oldest segment instead of waiting for the retransmission timeout
(RTO, minimum 1 s in smoltcp unless configured). What happens next is "recovery": how the sender repairs any
*other* holes in the same flight and how fast it may keep sending. smoltcp main does one repair per loss event;
every further hole in that flight waits for an RTO. On a Wi-Fi link with a shallow router queue, one burst often
loses several segments at once, which turns into the 1–2 s stalls we measured (`docs/TCP-WORKAROUND-30.md`).

**Validating your branch locally.** I can point our bottleneck benchmark (`forward_throughput` with
`BW_MBPS`/`QUEUE_KB`) and the review-31 randomized socket harness at a smoltcp checkout of yours, via
`[patch.crates-io]`, and compare against stock 0.14 and our patched build. Reviewing and testing are allowed under
their policy.
