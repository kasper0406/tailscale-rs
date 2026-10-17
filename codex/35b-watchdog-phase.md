# Brief 35b: make the map-poll watchdog say *what* was stuck

Follow-up to your brief 35 (`fix/control-watchdog`, worktree `/Volumes/git/ts-exit/scratch/control-watchdog`,
commit f9e6db2; GeiserX PR #634). The watchdog's timeout log is only "map response long-poll timed out!
Restarting control connection", so a recurrence wouldn't tell us which await hung. Kasper wants to be able to fix
the underlying bug from logs alone.

1. Track the current phase of a poll attempt in the watchdog (cheap: a small enum plus an `Instant`, e.g. in a
   `watch` channel or `Mutex` the watchdog already owns), set before each covered await in `run_once`: connect,
   register, map response headers, startup map request, reading a frame (also note whether a partial frame is
   buffered), C2N ping handling, each kind of side map request (name it), and processing a response. Use the
   real await sites (`ts_control/src/tokio/client.rs` ~730–933).
2. On timeout, the **warn** log (release builds strip debug) includes: the phase, seconds spent in it, seconds since
   the last map response, frames received in this attempt, and the attempt's age. Keep the existing message text as
   a prefix so it stays greppable. Never log keys, tokens, URLs with secrets, or peer data.
3. A paused-clock test per representative phase (at least: map headers, side request, reading frame with a
   partial prefix) asserting the logged phase. Use a tracing test subscriber, or expose the phase for the test.
4. Add it as a new commit on `fix/control-watchdog` (don't rewrite f9e6db2, since PR #634 is open), with trailers as
   before. Run `ts_control`/`ts_runtime` tests, clippy `-D warnings`, fmt. Don't push or touch the live node.

Report briefly by appending a "35b" section to `docs/CONTROL-WATCHDOG-35.md`.
