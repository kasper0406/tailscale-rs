# Brief 25 — direct-path recovery after restart

2026-10-13. Fix: `8640eb7` on `fix/restart-direct`, based on `ts-exit/fixes` (`1fe63d2`).
Worktree: `/Volumes/git/ts-exit/scratch/restart-direct`.

**Found and fixed a deterministic probe-starvation bug.** A UDP send error aborts the
whole discovery sweep, after all its targets have already been stamped as pinged. A
reachable laptop endpoint later in the sweep can therefore go unprobed indefinitely.
This reproduces the asymmetric direct/DERP state. **Attribution of the live incident
remains pending:** no live process, logs, state directory or tailnet was accessed.
The failing endpoint and the laptop's actual key-propagation timing are unobserved.

Code references below are relative to the fix worktree.

| Question | Evidence / answer |
|---|---|
| Does the restarted node probe unchanged endpoints? | Yes, it schedules them: `ts_runtime/src/direct.rs:676` runs `send_pings` every 2s; reconcile at `:1431` supplies the netmap endpoints. `ts_magicsock/src/sock.rs:1080` snapshots and stamps every eligible target before emitting. Scheduling does **not** prove the datagram left the socket. |
| Which disco key? | `sock.rs:1160` seals with the node's current `our_disco`, addressed to the peer's active disco key. `ts_keys/src/keystate.rs:220` regenerates disco state when loading persisted identity. Go `handleDiscoMessage` drops unknown sender keys and frames sealed to an old recipient key. A stale laptop key window is possible and simulated in the test; it cannot by itself explain persistent failure after propagation. |
| Does a failed round silence candidates? | No permanent cadence backoff: `path.rs:725`, `:511`, `:570` keep an untrusted peer eligible for full discovery. Only the 5s candidate floor applies; with 2s ticks, retries occur every 6s. The ≤5ms quiet-down and 60s upgrade gate require a trusted best. The new virtual-time test verifies 151 rounds over 15 minutes without any netmap update. |
| Why do retries fail? | Before the fix, `emit_disco_pings` (`sock.rs:1153`) used `send_udp(...).await?`. One unreachable endpoint aborts all remaining candidates **and peers**, despite their `last_ping` stamps. Netmap endpoints are unfiltered (`:948`), so an IPv6 endpoint can reach an IPv4-only socket. Stable HashMap iteration repeats the same failing prefix. Forced CallMeMaybe/rotation sweeps share this emitter and can also abort. |
| Does an inbound laptop ping help? | `handle_disco` (`sock.rs:2614`) verifies the disco/node binding, learns acceptable routable sources through `add_peer_endpoints`, then answers with a pong. This seeds a candidate, but does not establish outbound trust: a matching pong to our own ping is required (`path.rs:671`). Thus the laptop can confirm direct while the node stays on DERP. Learned private/loopback/disabled-IPv6 candidates are filtered; control candidates follow a separate authoritative path. |
| Does the node send CallMeMaybe? | Yes: `direct.rs:975` runs every 5s for unconfirmed peers, once a STUN/reflexive self endpoint exists and a DERP region is available. This is existing code, not a missing sender. The first periodic STUN sweep waits 20–26s; actual live CallMeMaybe delivery was not observed. |
| What changes for a new/reopened laptop? | New endpoints/key rebuild or alter candidate state and sweep order; endpoint reachability can change too. Key rotation also forces immediate probing (`direct.rs:1368`, `:1443`). That can bypass the old failing ordering. D29's “only probes on netmap change” describes the symptom, not the periodic scheduler's actual behavior. |

Go comparison: local `repos/tailscale-go` at `3bb8a906`. In
`wgengine/magicsock/endpoint.go:1357`, `startDiscoPingLocked` dispatches each ping
independently. `sendDiscoPing` at `:1314` handles a failed send for that transaction;
`sendDiscoPingsLocked` at `:1428` continues across endpoints and requests CallMeMaybe.
`magicsock.go:2188` rejects unknown disco keys; `handlePingLocked` at `:2560` learns
authenticated source addresses. The fix adopts the independent-send behavior,
without changing cadence, trust, binding checks or key persistence.

The production change is one error-handling block in `sock.rs`: count successful
sends, keep TCP-443-only refusals silent, log candidate send errors at debug level,
and continue. Failed in-flight records retain the existing bounded 5s pruning.

**Regression and validation.** `restart_direct_recovers_despite_a_failed_candidate`
(`sock.rs:2904`) uses encrypted disco frames over loopback, keeps the laptop socket
and state, rebinds the node on the same port with the same node key and a new disco
key, drops an early probe during stale-key propagation, then confirms laptop→node.
It fixes the emitter's candidate order to place an unsupported IPv6 address before
the working IPv4 address; this makes the failure deterministic without relying on
random HashMap ordering. Before the fix it fails with `Io(HostUnreachable)`; after
the fix both sides confirm direct. The separate cadence test at `sock.rs:2880`
uses `203.0.113.25` and virtual instants; it sends no network traffic.

| Inspected base | Same defect? | Patch validation |
|---|---|---|
| `ts-exit/fixes` `1fe63d2` | Yes | 186 magicsock tests + doc tests; clippy all targets with `-D warnings`; workspace fmt check. |
| GeiserX `origin/main` `3951752` | Yes, identical early return in `emit_disco_pings` | Patch applies unchanged; 184 magicsock tests + doc tests; same clippy/fmt checks. Scratch worktree `restart-direct-origin`. |
| Official `official/main` `b44db0d` | This defect is absent | No `ts_magicsock` crate. `crates/ts_runtime/src/path_discoverer.rs:250` repeats discovery every 5s and publishes each probe separately; `direct.rs:224` handles each `SendDisco` failure independently. Source comparison, not a certification of all upstream restart behavior. |

Commands: `cargo test --locked -j 4 -p geiserx_ts_magicsock`,
`cargo clippy --locked -j 4 -p geiserx_ts_magicsock --all-targets -- -D warnings`,
`cargo fmt --all -- --check`. All were invoked with `nice -n 10`; this sandbox
rejected `setpriority` (“Operation not permitted”) but ran the commands. Build
concurrency stayed at four jobs. No remote fetch/push or tailnet operation occurred.

**Claude hand-off.** `runner/restart-direct-check.sh PEER [-- NODE_ARGS...]` restarts
the configured running node using `stop.sh`, the settings in `env.sh`, and
`detach.py`. It uses the existing identity with `--interactive`, requests 1s status
lines, ignores all pre-restart log bytes, matches the exact displayed peer name,
and reports elapsed seconds from launch to the first node-side `path=direct`.
It exits 0 on success, 1 on failure/120s timeout, and 2 for invalid setup. It prints
no endpoint addresses and leaves the restarted node running. Preserve any custom
tag/hostname/port environment and supply other nondefault node options after `--`;
do not supply another `--status-interval`.

An isolated copy passed a mock detached restart (old status ignored, other peer
ignored, ANSI stripped), plus a virtual-clock 120s timeout. The actual live script
was **not run**. For the fixed build, a detached scratch worktree
`scratch/restart-direct-live` contains `ts-exit/main` `51ee63d` plus the fix
(`d3513ce`), leaving the live checkout unchanged. Its exit-node binary was built
successfully with `nice -n 10 cargo build --locked -j 4 --profile exitnode
--features ring-aead --example exit_node`:

```sh
RUST_LOG=info,ts_magicsock=debug \
RESTART_DIRECT_BIN=/Volumes/git/ts-exit/scratch/restart-direct-live/target/exitnode/examples/exit_node \
  /Volumes/git/ts-exit/runner/restart-direct-check.sh LAPTOP_DISPLAY_NAME
```

Use the actual displayed laptop name. Compare the node-side recovery time against
the reported pre-fix stall; the laptop's own direct status is insufficient. The
debug message `sending disco ping; continuing discovery` identifies isolated send
failures if present, so Claude can check the live attribution as well as recovery.
