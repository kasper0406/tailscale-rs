# Brief 35 — control map-poll watchdog

2026-10-17. Branch `fix/control-watchdog`, based on fetched GeiserX `origin/main`
`f36c757`. Worktree: `/Volumes/git/ts-exit/scratch/control-watchdog`.

## Finding

**The reported build already has a body-read watchdog. The exact live failure is not proven.**
Both `92c7b2e` and current GeiserX main contain the 120-second read timeout and HTTP/2
PINGs, introduced by `96f9d9f` (#109). A simple half-open connection while awaiting
body bytes should already time out and log. The confirmed gaps are waits outside
that reader, an extended deadline for partial frames, and missing control health.

Read-only inspection of `~/Library/Logs/rust-exit/exit_node.log` found no control,
HTTP/2 or watchdog messages during 00:00–00:59 UTC. Status at 00:00:59, 00:05:59,
00:10:59 and 00:15:59 reports `health=[]`. This supports the reported silence; it
does not establish a NAT expiry or identify the pending future. No live process
inspection, daemon changes or restart was performed.

References below describe the original code at `92c7b2e` (also present on main):

- `ts_control/src/tokio/client.rs:753` requests `KeepAlive=true`.
  `map_stream.rs:446` decodes the flag and yields keep-alives; `client.rs:387`
  excludes them only from **backoff reset**, not from consumption.
- `map_stream.rs:308,339` separately time out the length and body reads after
  120 seconds. A prefix arriving at 119 seconds therefore extends the wait to
  about 239 seconds without a complete response.
- `client.rs:769` awaits map response headers before creating the reader.
  `client.rs:837,870,892` awaits C2N replies and side map requests inside the
  stream/command loop. While an arm is awaiting, the reader and its timer are
  not polled. `ts_http_util/src/http2.rs:45` also holds the request mutex until
  response headers arrive. A stalled HTTP request can therefore silence the
  reader without triggering its watchdog. This is a plausible incident mechanism,
  not a confirmed diagnosis of the live process.
- `client.rs:658–689` retries both errors and EOF with the existing jittered
  backoff and session cursor. Those logs run only after the attempt returns;
  a forever-pending attempt produces neither an error nor a reconnect log.
- `ts_http_util/src/http2.rs:83` already configures 60-second PINGs, a 20-second
  ACK timeout and PINGs while idle. A responsive HTTP/2 layer can ACK PINGs while
  an application request hangs. Neither `ts_control/src/control_dialer.rs:95`
  nor `ts_http_util/src/lib.rs:181` explicitly configures TCP keepalive.
- `ts_runtime/src/lib.rs:731` builds health only from the packet-filter flag
  (`packetfilter.rs:69`). Thus `health=[]` says nothing about control liveness.

Go reference: local `repos/tailscale-go` at `3bb8a906`,
`control/controlclient/direct.go:1018,1170,1266,1299,1332`: 120-second watchdog,
initial request coverage, response-based resets, and keep-alive consumption.
Go pauses its timer during response processing and handles ping replies separately.

## Fix and tests

[MapPollWatchdog](../scratch/control-watchdog/ts_control/src/tokio/map_watchdog.rs)
supervises the entire reconnect attempt, including connect/register, map headers
and awaited side work. Every decoded response, including `KeepAlive`, resets the
120-second deadline before processing. Timeout logs at **warn**, cancels the
attempt and drops its HTTP/2 connection; the existing loop reconnects. No new
dependencies or transport settings were added. Unlike Go's processing pause,
awaited side work is also bounded because it currently suspends this reader.

The reader now shares one deadline across length and body. Connection health
becomes false on EOF, error or watchdog expiry, remains false during retries,
and clears only on a map response. `Runtime::status().health` adds
`Disconnected from coordination server; retrying map poll` alongside any existing
packet-filter warning.

Paused-clock regressions cover:

- Prefix at 119 seconds followed by a stalled body: failed on unmodified main
  at the 121-second test bound; now ends at 120 seconds. The three existing
  plain-stall reader tests already passed on main.
- A real framed duplex reader left open without further bytes, and a poll
  suspended in simulated side work: cancellation at 120 seconds, resource drop,
  second fake attempt before 121 seconds, and health recovery on its response.
  This exercises the production watchdog and backoff helper without live control.
- Stalled setup before a reader exists; four minute-spaced keep-alives extending
  liveness to 360 seconds before timeout; immediate health changes on EOF/error;
  preservation of simultaneous packet-filter warnings.

A retry after a substantive map is immediate. Repeated failed polls retain the
existing backoff (under 45 seconds after jitter); successful reconnection still
requires reachable control. Server-directed registration cooldowns remain intact.

## Official tailscale-rs

Fetched `official/main` = `0d41a6f`. **It has the broader gap.**
`crates/control_plane/ts_control/src/client/map_stream.rs:114–140` awaits length
and body without a watchdog; `:266` awaits headers without a timeout. It requests
keep-alives (`map_request_builder.rs:77`) and decodes their JSON but retains no
heartbeat/liveness flag. `crates/utils/ts_http_util/src/http2.rs:69` uses the plain
handshake without PING configuration. `crates/ts_runtime/src/control_runner.rs:405`
attaches the reader; `:441` stops on stream completion, which silence never causes.
Official was inspected only; this branch targets GeiserX.

## Validation

Local commit: `f9e6db2d2288103d982a7f13ac1c40989bc34f19`, with the requested
Signed-off-by and Co-Authored-By trailers. Worktree clean; nothing pushed.

All checks passed using Rust 1.95.0:

```sh
cargo test --locked -p geiserx_ts_control -p geiserx_ts_runtime --all-features
cargo clippy --locked -p geiserx_ts_control -p geiserx_ts_runtime --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Results: 391 control + 649 runtime + 1 integration test; doc tests passed
(no cases). Regression entry points:
`map_stream_watchdog_uses_one_deadline_for_length_and_body` and
`watchdog_reconnects_stalled_reader_or_side_rpc_and_recovers_health`.
Logs: `scratch/control-watchdog-{baseline,tests,clippy,fmt-check}.log`.

`CARGO_BUILD_JOBS=4` and `RUST_TEST_THREADS=4` bounded compilation and tests.
All Cargo commands used `nice -n 10`, but the sandbox rejected `setpriority`;
the requested priority could not be applied. A writable Cargo home at
`scratch/control-watchdog-cargo-home` avoided writes to the restricted home
cache. `bd` was unavailable; no tracker changes were made. No public addresses
were copied into this report or the new tests.

## 35b — identify the stalled await

New commit `b66def8a0d5424127eca8fd39eac69a40aab7f1c` on
`fix/control-watchdog`, directly after unchanged `f9e6db2`; requested trailers
included. Nothing pushed; live node untouched.

The existing watch channel now records connect, register, map response headers,
frame reads, response processing, C2N ping handling and each named side request:
`set_derp_home_region`, `set_endpoints`, `set_routable_ips`, `set_hostname`.
Startup Hostinfo runs concurrently, so it appears as
`concurrent_phase=startup_map_request` with its own age rather than replacing
the reader's phase.

Both watchdog timeout paths retain the **warn** message prefix
`map response long-poll timed out! Restarting control connection` and include:

- `phase`, `phase_secs`, `since_last_map_response_secs`, `frames_received`,
  `attempt_age_secs`;
- `partial_frame`, `frame_part` (prefix/body), bytes received/expected;
- concurrent startup phase and `startup_request_secs`.

Partial-byte progress comes from actual reads and never extends the deadline.
Decoded frames, including keep-alives, are counted before cache persistence;
response arrival age therefore remains useful if persistence hangs. Before the
first response, response age is attempt age. Re-entering an unchanged phase does
not reset its age. Diagnostics contain only fixed labels, durations and counts;
no keys, tokens, URLs or peer data. Timeout and health behavior remain unchanged.

Seven new paused-clock tests inspect the same snapshot used by the warn log:
headers, all four side requests, a two-byte prefix at 119 seconds, partial body
versus silence, concurrent startup/reader and startup/processing states, and
single counting of decoded keep-alives/netmaps.

Validation passed with the commands above: **398 control + 649 runtime + 1
integration test**, doc tests, clippy `-D warnings`, fmt and whitespace checks.
Logs: `scratch/control-watchdog-35b-{tests,clippy,fmt-check}.log`. Jobs remained
capped at four; `nice -n 10` again encountered the sandbox's `setpriority` denial.
