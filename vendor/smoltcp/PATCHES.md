# Private smoltcp 0.14.0 recovery workaround

Base: the published crates.io **0.14.0** source, checksum
`b6f8b28ad56c6e35524a37dd492af5d1a47e31e1a4d175cd12f89c075f01980f`.
The import is separate from the patches. The repository pins the source, uses
`=0.14.0` requirements and `[patch.crates-io]`; the workspace lockfile selects this
path. License: 0BSD. No floating Git dependency or additional production dependency.

Local changes:

- **B1:** keep a fast repair pending after failed emission and retain a fallback RTO.
- **B2:** restore the outstanding-flight RTO after a zero-window reopen. Also keep
  persist active when an RTO expires with the window closed and queued data remains.
- Backport upstream **51c662c / #1211**'s `set_min_rto` and `set_initial_rto` APIs,
  reset preservation and tests, retaining 0.14's `u32` storage. Library defaults stay 1 s.
- NewReno: original recovery boundary, next-hole repair on partial ACK, once-per-episode
  loss entry and partial deflation in both existing controllers. Fresh sends remain
  bounded by cwnd; full ACK or RTO ends recovery.
- Opt-in **32-range sender SACK**, including **4e6b554**'s flight-relative input
  validation, bounded walks and hostile-state regressions. Only cumulative ACK frees
  data; RTO clears advisory ranges. Pipe counts fresh data too. SACK recovery holds
  the controller's loss threshold rather than applying Reno duplicate-ACK inflation
  or NewReno partial-ACK deflation. This is an RFC 6675 subset, without rescue or RACK.
- Opt-in pacing at **1.2 × cwnd / SRTT**, with bounded bursts and saturating debt
  arithmetic. It requires a valid RTT estimate and baseline RTT above 5 ms. ACKs and
  first fast repair stay immediate; RTO, delayed ACK and timeout deadlines retain
  priority. Library pacing and sender SACK defaults remain disabled.
- Core-compatible Cubic multiplication replaces `powi`/`mul_add`, preserving the
  existing algorithm and cube-root accuracy test. Minor Rust 1.95 lint/format changes
  are isolated in a style commit.

The netstack policy is a separate commit: explicit Cubic, sender SACK, pacing and
200 ms **computed** RTO floor. Initial SYN RTO remains 1 s. The buffer default remains
256 KiB per direction; the recommendation is an explicit 1 MiB deployment override.
See the Brief 30 report and raw matrix for the measured policy choice.

```sh
nice -n 10 cargo +1.95.0 bench --locked -j 4 -p geiserx_ts_forwarder \
  --bench forward_throughput --no-run
nice -n 10 cargo +1.95.0 test -j 4 --manifest-path vendor/smoltcp/Cargo.toml \
  --lib --features socket-tcp-reno,socket-tcp-cubic,assembler-max-segment-count-32
nice -n 10 cargo +1.95.0 test -j 4 --release \
  --manifest-path vendor/smoltcp/Cargo.toml --lib --no-default-features \
  --features std,medium-ip,proto-ipv4,proto-ipv6,socket-tcp,socket-tcp-reno,socket-tcp-cubic,assembler-max-segment-count-32 \
  --no-run
# Run the reported test executable for seeds 11, 22, 33, 44, at most four processes:
R_ROUNDS=100000 R_STEPS=400 R_SEED=11 R_PROGRESS=1 nice -n 10 <test-binary> \
  socket::tcp::test::review26b_random_peer_inputs --exact --ignored \
  --nocapture --test-threads=1
```

The randomized test is the review-26 harness port: same PRNG and draws, 400 steps,
MSS/reordering/window/hostile SACK inputs and 20,000-step cooperative drain bound.
Disable the `log` feature for the long run to avoid per-packet test logging; the
features above do that. This affects instrumentation, not the socket algorithm.
