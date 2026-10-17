# Review 34: what does our evidence actually show about pacing vs loss recovery? (read-only, skeptical)

You are an independent, skeptical reviewer. You did not build any of this; assume nothing in our reports is right
until the data supports it. **Read-only: write only your report.**

## Context

Kasper runs a Rust userspace Tailscale exit node (smoltcp TCP stack terminates every forwarded client TCP flow,
then splices to the host's kernel TCP). For client downloads, smoltcp is the TCP *sender* towards the client.
Claude has told Kasper two contradictory things:
1. "Pacing is the biggest single gain", based on emulated benchmarks.
2. After one real-world run showed no pacing effect: "for ordinary exit-node use, loss recovery (NewReno/SACK/
   200 ms min RTO) matters more than pacing".
Kasper is skeptical of the initial benchmarks and of both claims. He wants to know what the evidence supports,
and is considering filing a smoltcp issue proposing pacing (and possibly writing NewReno/SACK himself).

## Evidence (all in /Volumes/git/ts-exit)

- Emulated bench: `ts_forwarder/benches/forward_throughput.rs` (repo `repos/geiserx-tailscale-rs`, branches
  `ts-exit/tcp-0.14`, `ts-exit/pacing-ab`; worktrees `scratch/tcp-0.14`, `scratch/pacing-ab`). Both endpoints are
  smoltcp; an in-memory relay adds delay, a bandwidth limiter and a tail-drop byte queue.
- `docs/TCP-WORKAROUND-30.md` (stock vs mitigations vs our patched recovery, with/without pacing, raw JSONL under
  `scratch/tcp-0.14/docs/`), `docs/PACING-AB-32.md` (stock 0.14.0 vs stock + pacer only, 432 runs, raw data under
  `scratch/pacing-ab/docs/`), `docs/TCP-RECOVERY-24.md`, reviews `docs/review-26-tcp-recovery.md`,
  `docs/review-31-tcp-0.14.md`.
- Real-world runs from Kasper's laptop on remote Wi-Fi through the node: `docs/DECISIONS.md` F31, F32, F35, F37.
  Today's pacing A/B (F37): laptop output `bench/results/pacing-real/laptop-pacing-real-1.txt`, node counters
  `bench/results/pacing-real/node-stats-*.jsonl` (cumulative, node-wide, one sample per second), analysis script
  `runner/pacing-real-analyze.py`, counter definitions `docs/PACING-LIVE-33.md`, the experiment scripts
  `deploy/laptop/run-pacing-sched.sh`, `runner/pacing-schedule.sh`.

## Questions (cite files/lines/numbers; separate what's measured from what's inferred)

1. **Is the emulated bench a fair test of pacing?** Look for biases: both ends smoltcp (receiver behaviour,
   delayed ACKs, window updates, ACK pacing), how bursty the relay and the sender really are compared with a real
   path (no WireGuard, no NIC/driver, no ACK aggregation as on Wi-Fi), whether the queue sizes and windows chosen
   favour pacing, whether stock smoltcp's *own* bugs (1 s min RTO, no NewReno/SACK, B1/B2) inflate the measured
   benefit (pacing avoids losses that stock recovery handles badly), the no-loss controls (why does pacing gain
   12–48% at RTT 40 ms with no loss?), measurement artefacts (15 s runs including startup, one machine,
   repetition counts), and anything else. Is "51 → 112 Mbit/s median" a meaningful number?
2. **What does the real-world run (F37) actually show?** Did pacing engage on the measured flows (deferral
   counts, the listener-socket caveat, timing alignment)? Are the counters attributable to the test (node-wide,
   other devices)? Is the ~21 Mbit/s single-flow ceiling explained by RTOs/loss, by the 1 MiB window and RTT, by the
   access link, or something else? Can "losses aren't caused by our bursts" be concluded? What's the statistical
   power of 3+3 blocks?
3. **Claims check:** grade each of Claude's statements (pacing biggest gain; recovery more important for ordinary
   use; "pacing is situational / deep buffers absorb bursts"; the earlier real-world 9 → 17–20 MB/s improvement
   attributed to the patched build) as supported / weakly supported / unsupported, with reasons.
4. **What should Kasper believe and do?** An evidence-based ranking (B2, NewReno, SACK, pacing, min RTO) for an
   exit node serving phones/laptops, with confidence levels. And the **smallest decisive experiments** to settle
   the open questions in the real world (e.g. stock vs patched recovery on the laptop path; a real shallow-buffer
   step-down on the LAN with ACK-path delay; others), with what result would support or refute each claim.
5. For the smoltcp pacing issue Kasper drafted: what can it honestly claim today?

Write `/Volumes/git/ts-exit/docs/review-34-evidence.md`: a short verdict up front, then the answers. Be blunt
where the evidence is weak. Concise.
