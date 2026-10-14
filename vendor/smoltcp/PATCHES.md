# Private smoltcp 0.14.0 workaround

Base: published crates.io 0.14.0, checksum from the pre-patch Cargo.lock:
`b6f8b28ad56c6e35524a37dd492af5d1a47e31e1a4d175cd12f89c075f01980f`.
Source and lockfile are pinned by this repository commit; no floating Git dependency.
The source import is separate from each local patch. License: 0BSD.

This private series preserves upstream congestion controllers and defaults.
Patches: failed-emission retry, zero-window reopen timer, upstream RTO-bound API,
NewReno partial ACK, bounded sender SACK (review-26 input validation and walk bounds).
Application policy is committed separately. See the Brief 30 report for measurements.
