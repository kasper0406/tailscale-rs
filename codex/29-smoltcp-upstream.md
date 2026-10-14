# Brief 29: should our smoltcp TCP sender work go upstream, and as bug fixes or features? (read-only)

Context: our exit node terminates forwarded client TCP in smoltcp. On a real Wi-Fi client (~25 ms, 50–250 Mbit/s)
the sender stalled for seconds after burst loss at a shallow bottleneck queue; with a 2 MiB window a single
download collapsed to 0.3 MB/s. Codex brief 24 fixed it in our vendored smoltcp **0.13.1**
(`docs/TCP-RECOVERY-24.md`, `vendor/smoltcp/PATCHES.md`, `docs/review-26-tcp-recovery.md` incl. the SACK
validation fix 4e6b554 found by review). The commits are in `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`
on tag `archive/ts-exit/perf-pre-geiserx-switch` (search `git log --oneline` for `smoltcp:`), and on branch
`ts-exit/tcp-recovery` with the bottleneck bench (`ts_forwarder/benches/forward_throughput.rs`, `BW_MBPS`/`QUEUE_KB`).
GeiserX now uses crates.io smoltcp **0.14.0** and pins Reno (see `docs/review-28-design.md` section 1, which
notes 0.14 already has flight-size cwnd accounting, Reno ACK/RTO corrections and zero-window probes, but no
sender SACK/NewReno and a 1 s RTO floor).

Kasper asks: is this a feature or a bug fix, and does it make sense to PR to smoltcp (smoltcp-rs/smoltcp)?

## Do (read-only; no PRs, comments or pushes; you may clone smoltcp into `/Volumes/git/ts-exit/scratch/`)

1. Clone current `smoltcp-rs/smoltcp` main. For **each** of our smoltcp commits, decide: already fixed upstream
   (cite the upstream commit/PR), still a **bug** on upstream main (show the behaviour: a failing test or a
   precise code trace on main, e.g. go-back-N after fast retransmit, recovery-boundary loss, Cubic slow-start or
   unit bugs, RTT sampling during recovery), or a **feature/policy** (sender SACK scoreboard, NewReno partial-ACK
   as an RFC 6582 improvement, pacing, per-socket min RTO). Where it's a bug, try to reproduce on upstream main
   with a minimal smoltcp unit test (in the scratch clone only).
2. Survey upstream: open/closed issues and PRs on SACK, NewReno, fast recovery, congestion control, pacing, RTO
   floor; CONTRIBUTING/README guidance; how maintainers responded to similar work; release cadence; MSRV, no_std
   and `alloc`-free constraints, feature-flag conventions (e.g. `socket-tcp-cubic`), and how much code they accept
   in one PR. Note whether the sender SACK scoreboard fits their no_std/fixed-size philosophy (ours uses
   `heapless::Vec<_, 32>`).
3. Recommend: which items to PR, in what order and size (bug fixes first?), what each PR needs (tests, benches,
   RFC references, no_std/32-bit checks, the SACK-validation hardening from review 26), and which to keep local
   or drop. Also say whether a short issue first (to gauge interest in sender SACK/pacing) is wiser than a PR.
   Estimate the porting effort onto main.

## Deliverable

`/Volumes/git/ts-exit/docs/SMOLTCP-UPSTREAM-29.md`: a table (our commit → upstream status → bug/feature →
recommendation), the evidence for each "still a bug" (test or trace), the upstream survey, and the recommended PR
plan. Concise. No public IP addresses.
