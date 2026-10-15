# Brief 30b: fix review 31's LOW-1 (unbounded pacing delay) on ts-exit/tcp-0.14

An independent review of your branch (`docs/review-31-tcp-0.14.md`) found it safe to run live, with two LOW
findings. Please fix **LOW-1** in the worktree `/Volumes/git/ts-exit/scratch/tcp-0.14`:
`vendor/smoltcp/src/socket/tcp/pacing.rs:51-57`: fast repairs bypass the pacing budget check but still advance the
next-send time, so repeated partial ACKs plus a peer-inflated RTT can push a flow's next send many seconds ahead
(8.3 s release, 30 s debug fuzz). Clamp the scheduled next-send time to about `now + srtt` (or another bound you
can justify), add a regression test that fails before the fix, and keep everything else unchanged.

LOW-2 (Pipe recomputed per `poll_at` with a full 32-range scoreboard, ~1.9 µs) is optional: fix it only if it's
small and clean (e.g. compute Pipe once per dispatch); otherwise leave it and say so.

Then rerun the reviewer's extended harness, which covers wraparound, failed sends, pacing, partial ACKs and a
poll_at-driven drain, from `/Volumes/git/ts-exit/scratch/review31-copy/vendor/smoltcp/src/socket/tcp/review31.rs`
(see its run logs `run31_seed*.txt` for the commands) against the fixed code. Use at least 4 × 100k release
rounds plus 1 × 100k with overflow checks, and report the maximum pacing delay observed. Also rerun the smoltcp
library tests, the netstack/forwarder tests, clippy `-D warnings` and fmt.

Commit as a separate `smoltcp:` commit (trailers as before). Don't touch the live node, `ts-exit/main` or remotes.
Append a short "30b" section to `docs/TCP-WORKAROUND-30.md` with the result.
