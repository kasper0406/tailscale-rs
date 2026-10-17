# NAT port mapper (NAT-PMP + PCP): implementation report

Status: phases 0 and 1 of `docs/PORTMAPPER-PLAN-v2.md` are implemented on `feat/portmapper` and pass
every local gate. **Not yet field-validated, and not yet deployable as-is.** The branch is based on
`origin/main`, so it lacks this fork's `--serve-tcp` and `--interactive` flags (see §6b). Bead
`tsr-x4yg`.

- Worktree: `scratch/portmapper`, branch `feat/portmapper`, head **`5077267`**, which is 48
  commits on `origin/main` (`69713c9`). Nothing has been pushed.
- Binary: `bin/exit_node-portmap`, built from `07f4628`. The one later commit only touches the
  fuzz crate's lock file, so it doesn't change the binary. Build command:
  `cargo +1.95.0 build --locked --profile exitnode --features ring-aead --example exit_node`.
  SHA-256 `49a1ffc28978f5becb87ecdeb5411a517ed517a38ddf4c47863d4842cd330e31`. It contains no
  test-harness symbols (`nm` finds no `FakeRouter` or `TestCap`).
- Related docs: plan v2 (`docs/PORTMAPPER-PLAN-v2.md`), review 36
  (`docs/review-36-portmapper-plan.md`), and on the branch the user-facing
  `docs/PORTMAPPER.md` (off switches, deviations from Go, router-state bounds).

## 1. What was built

### 1.1 Shape
| Piece | Where | Role |
|---|---|---|
| `ts_portmapper` crate (`geiserx_ts_portmapper`) | `ts_portmapper/src/` | Codecs, gateway lookup, `GatewayCap`, the one-shot client, the lifecycle `Client`, the schedule and the owned-rule ledger |
| Strict codecs | `pmp.rs`, `pcp.rs` | RFC 6886 and 6887, IPv4 only. Internal port 0 can't be expressed. PCP nonces come from a CSPRNG. Every reply field is validated. |
| Gateway lookup | `gateway/{mod,darwin,linux}.rs` | macOS `sysctl(NET_RT_DUMP2)` with a bounds-checked parser; Linux `/proc/net/route`. The single lowest-metric default route must be private and on-link; there is no fall-through. Returns `Result<Option<Gateway>, LookupError>`. |
| `GatewayCap` | `cap.rs` | The only code that creates sockets or sends. The only destination is `LanDest::Pxp` (`gw:5351`). Sockets are bound to the host's own IP and interface (`IP_BOUND_IF` / `SO_BINDTODEVICE`, else source-IP mode with kernel-route revalidation) with `SO_DONTROUTE`. Policy and validity are checked before every send. Receives use a 250 ms absolute deadline, an exact source, a cap+1 buffer and at most 64 datagrams examined. |
| One-shot client | `oneshot.rs` | `probe` / `map` / `delete`. The lifecycle client does all its I/O through these, so there is no second mapper. |
| Lifecycle client | `client.rs`, `schedule.rs`, `owned.rs` | Generation-guarded state under one mutex, never held across an await. Single-flight with an RAII reset. `next_attempt_at`, `renew_after` and `expires_at` are kept apart. Ledger of at most 4 owned rules. Ordered `close(budget)` and a Drop-path release. `check_network` and `check_clock` detect a new network and suspend. |
| Runtime driver | `ts_runtime/src/portmap.rs`, `direct.rs` | Spawned by `DirectManager`. Wakes on a 10 s route poll (independent of the idle gate), local expiry, renewal (runner mode), ReSTUN ticks and kicks. Invalidates synchronously on rebind, failed rebind and `only-tcp-443`. `PortmapRelease` guard: `close()` on graceful shutdown, a best-effort delete on Drop. |
| Endpoints and control | `ts_magicsock/src/sock.rs`, `ts_control/src/net_facets.rs`, `control_runner.rs` | `PortMapped` (wire type 3) comes first; addresses are de-duplicated with PortMapped winning. `StunFacts` and `PortMapFacets` are carried separately. `PMP`/`PCP`/`HavePortMap` go on every map request. The CallMeMaybe gate counts a port mapping. |
| Config and knobs | `src/config.rs`, `ts_control/src/config.rs`, `ts_runtime/src/env.rs` | `disable_port_mapper` and `PortMapperOptions { gateway_override, advertise_nonpublic_external, renew_while_idle }`. `TS_DISABLE_PORTMAPPER` is read once at spawn. The cargo feature `portmapper` is on by default. |
| Runner | `examples/exit_node/main.rs` | `--portmap-probe` and its options; `--no-portmapper`, `--portmap-advertise-nonpublic`, `--portmap-lazy-renew`, `--portmap-gateway`. The status line gains `portmap=` and `ext=`. |
| CI checks | `checks/src/portmapper_lan_only.rs`, `portmapper_test_isolation.rs` | Dependency and feature graph, LAN-only tripwire with adversarial fixtures, test-isolation gate |
| Test harness | `ts_portmapper/src/testing/` (`cfg(test)` or feature `test-harness`) | `FakeRouter` with its own RFC decoder, loopback only; `TestCap`/`TestCaps`, scripted lookup, manual clock |
| Fuzzing | `ts_portmapper/fuzz/` | `parse_pmp`, `parse_pcp`, `parse_pcp_map`, `parse_rt_dump`, `parse_proc_net_route` |

### 1.2 Commits (`origin/main..feat/portmapper`, oldest first)
Every commit carries `Signed-off-by: Kasper Nielsen` and `Co-Authored-By: Claude Opus 5.5` (checked
on all 48).

**Phase 0** (plan §6, §10):
1. `d795d50` feat(portmapper): add the ts_portmapper crate skeleton
2. `8033537` feat(portmapper): add a strict NAT-PMP codec
3. `60fe1be` feat(portmapper): add a strict PCP codec
4. `c4a0d9f` feat(portmapper): select and validate the gateway
5. `8a0e7f2` feat(portmapper): read Linux default routes from /proc/net/route
6. `b9a087f` feat(portmapper): read macOS default routes from the routing socket
7. `8989b64` feat(portmapper): add GatewayCap, the only path to the LAN
8. `320af1e` test(portmapper): add the fake NAT-PMP/PCP router and TestCap
9. `06fdf55` feat(portmapper): add the one-shot probe, map and delete client
10. `e44739d` feat(exit_node): add --portmap-probe, a one-shot NAT-PMP/PCP check
11. `2987583` test(portmapper): add fuzz targets for every parser
12. `89f1a66` feat(checks): guard the port mapper's LAN-only boundary

**Phase-0 review fixes:**

13. `fdbcb30` fix(portmapper): don't read the routing table for a gateway override
14. `83d2027` test(portmapper): reply per request in the fake router; guard the deadline
15. `9ca2360` fix(portmapper): don't let a nonce-less PCP error end an exchange
16. `c16bfee` fix(portmapper): keep NAT-PMP address errors apart from MAP refusals
17. `00e1444` fix(portmapper): check kernel routing in source-IP mode; SO_DONTROUTE
18. `0094ed3` fix(portmapper): probe handles interrupts, late routes and short leases
19. `1627c2a` fix(portmapper): treat Linux ipv6_route as advisory
20. `a1f55c6` feat(portmapper): reject implausible external addresses
21. `7cc62db` feat(checks): widen the port mapper tripwire
22. `bc0ef58` test(portmapper): use documentation addresses in the address-class test

**Phase 1:**

23. `9744d06` feat(portmapper): add the retry, renewal and expiry schedule
24. `508e3c3` feat(portmapper): add the owned-rule ledger
25. `7f397f3` feat(portmapper): add the lifecycle client
26. `c6aafaa` feat(magicsock): advertise a port-mapped endpoint first
27. `f767a12` feat(control): carry STUN facts and port-map facets with endpoints
28. `3eed91f` feat(config): add disable_port_mapper and port-mapper options
29. `672a436` feat(runtime): run the port mapper for the underlay socket
30. `d794cb9` test: keep device-building tests off the LAN gateway
31. `a23fad4` feat(exit_node): port-mapper flags and a portmap= status field
32. `8d69723` docs: record the port mapper in the ledger and roadmap

**Phase-1 review fixes:**

33. `a265cb2` fix(portmapper): forget a retry request on the next attempt request
34. `b0d1c2f` fix(portmapper): end single-flight before an attempt's follow-up cleanup
35. `c217042` fix(portmapper): retry a failed create from the driver timer
36. `6a8d97b` fix(runtime): kick a port-map round when the poll finds a network
37. `447e6e4` fix(portmapper): keep a probe from rolling back a newer mapping
38. `f0673c3` fix(portmapper): age leases by the time the host slept
39. `68d6a35` fix(portmapper): hand a cancelled cleanup's claim back
40. `123a213` fix(portmapper): bound warn-once keys and split their budget
41. `75e1f6b` fix(portmapper): hold probes after one found no service
42. `e5da232` fix(runtime): tell the port mapper whether a socket is still bound
43. `17b473d` refactor(runtime): hold the port mapper behind a release guard
44. `3ad9aee` test(portmapper): regression tests for the phase-1 review
45. `fd527db` test(runtime): driver and release-guard regression tests

**Final verification (this report):**

46. `bad739f` test(portmapper): make the rollback-during-attempt test deterministic. It was
    flaky: about 1 run in 40 in isolation, and it failed once in three full-workspace runs.
47. `07f4628` test(portmapper): use a documentation address in the non-private self test. This
    replaces the `172.32.0.5` fixture with `192.0.2.5`.
48. `5077267` build(portmapper): refresh the fuzz crate's lock for tracing. Before this commit,
    `cargo fuzz` and any `--locked` build of `ts_portmapper/fuzz` failed.

## 2. Mapping to plan v2
| Plan v2 | Status | Notes |
|---|---|---|
| §3.1 crate layout | Done | Package `geiserx_ts_portmapper`. Phase 2 (UPnP) has no code yet. |
| §3.2 lookup contract (M5) | Done | Typed result, metric selection, `NotPrivate` with no fall-through, `GatewayIpv6`, `Ambiguous`, RTAX_IFA plus kernel-source tiebreak. Linux `ipv6_route` is advisory (`1627c2a`). |
| §3.3 `GatewayCap` (H3, departures c and d) | Done, and stricter | Adds `SO_DONTROUTE`, a prefix bound of /8–/31, and interface re-checks. Source-IP mode needs a lookup and checks `kernel_source(gw) == self_ip` before every send (`00e1444`). |
| §3.4 codecs (M8) | Done | Includes `is_plausible_external` (`a1f55c6`). The NAT-PMP address error is kept apart from a MAP refusal (`c16bfee`). A short PCP error is "unverified" (`9ca2360`). |
| §3.5 lifecycle and generations (H1) | Done | Ledger of 4 rules. Stale commits are orphaned and deleted. RAII single-flight. The cleanup claim is returned on cancel (`68d6a35`). |
| §3.6 schedule (H2) | Done | Backoff 10 s → 10 min with ±20 %. 2 s floor. NOT_AUTHORIZED holds ≥ 30 min. The PCP error lifetime is capped at 30 min. Probes obey the backoff (`75e1f6b`). |
| §3.7 network identity and epoch (M6) | Done | 10 s poll; a found network kicks a round (`6a8d97b`). Suspend ages the leases (`f0673c3`). Epoch is probe-only (departure e deferred). A probe can't roll back a newer mapping (`447e6e4`). |
| §3.8 endpoints, NetInfo, control (M7, departure a) | Done | `HavePortMap` means an *advertised* mapping. |
| §3.9 driver | Done | Bound-socket awareness (`e5da232`); the failed-create retry timer (`c217042`). |
| §3.10 ready for UPnP | Design only | No code, as planned |
| §3.11 config, knobs, runner flags | Done | Runner defaults: suppress non-public addresses, renew while idle with peers |
| §3.12 shutdown and release | Done | `close` budget `min(1 s, timeout)` inside the existing shutdown timeout. Drop path through `PortmapRelease` (`17b473d`). |
| §4 router-state bounds | Documented | `docs/PORTMAPPER.md` on the branch |
| §5 anti-leak enforcement | Done | `portmapper_lan_only` and `portmapper_test_isolation` are registered in `CHECK_FNS` |
| §6 phase 0, including the probe | Done (code) | Field validation is **not done**: see §6a |
| §7 phase 1 | Done (code) | Field check and overnight observation **not done**; deploy blocked on integration (§6b) |
| §8.3 harness | Done, one gap | The Linux-only real `127.0.0.2` two-router variant was not built. Two routers are covered through `TestCaps` logical gateways. |
| §10 commit plan | Followed | Plus review-fix commits |

### Review 36 findings → resolution
| Finding | Resolved by |
|---|---|
| H1: a stale create or renew resurrects a mapping | Generations, abort on invalidate, stale commit → orphan → delete (`7f397f3`, `508e3c3`). Tested by M1/M2 and the M1 variants in `3ad9aee`. |
| H2: renewal storm; no expiry timer | `schedule.rs` (`9744d06`); the driver's expiry wake whatever the activity; loss is published. Tested by M3/M4 and the bounded-datagram test in `3ad9aee`. |
| H3: "LAN only" is not a boundary | `GatewayCap` (`8989b64`, `00e1444`), `LanDest`-only API, checks (`89f1a66`, `7cc62db`). Tested by 24a/24, M7 and M9. |
| H4: router-state bounds overstated | Ledger, honest bounds in `docs/PORTMAPPER.md`, probe lease 120 s. Permanent UPnP policy fixed for phase 2. |
| M5: gateway lookup contract | `c4a0d9f`, `8a0e7f2`, `b9a087f`, `fdbcb30`, `1627c2a` |
| M6: stale network without netmon | 10 s poll, synchronous invalidation, network identity including the interface, `check_clock` (`672a436`, `6a8d97b`, `f0673c3`, `e5da232`) |
| M7: endpoint order and STUN evidence | `c6aafaa`, `f767a12`. Tested by cases 26–28 and M6. |
| M8: disable semantics, packet validation | Policy at every send and commit; `only-tcp-443` sends nothing, deletes included; strict codecs; CSPRNG nonce; port 0 rejected (`672a436`, `60fe1be`, `8033537`). Tested by case 23. |

## 3. Test inventory

### 3.1 Go tests
| Go test | Rust |
|---|---|
| `TestProbeIntegration` (PxP half) | Ported (`oneshot.rs`, Go's counter names) |
| `TestPCPIntegration` | Ported (`oneshot.rs` / `client_tests.rs`) |
| `TestUpdateEvent` | Adapted and strengthened (M5): success, loss, expiry and disable, for PMP **and** PCP |
| `TestParsePCPMapResponse` | Ported; the golden reply is rewritten into 203.0.113.0/24 |
| `FuzzParsePCPResponse`, `FuzzParsePCPMapResponse` | Fuzz targets plus seeds, with stronger invariants (MAP rejects ANNOUNCE) |
| netcheck `TestReportString` (portmap cases) | `PortMapFacets::summary()` plus `active-` |
| netmon `TestLikelyHomeRouterIP`, `_Prefix`, `_NoMocks` | Adapted (`gateway/mod.rs`). `_NoMocks` is a local read only. |
| netmon `TestExtremelyLongProcNetRoute`; Cloud Run and App Runner tables | Ported (`linux.rs`) |
| netmon `TestFetchRoutingTable` | Ported: 10 fetches agree (`darwin.rs`) |
| netmon `TestLikelyHomeRouterIPSyscallExec` | Adapted: agrees with `netstat -rn` (local exec) |
| magicsock `DisablePortMapper: true` | Case 31 plus the `portmapper_test_isolation` check |
| `TestCreateOrGetMapping`, `TestClientProbe`, `TestClientProbeThenMap` (live router) | Replaced by the manual `--portmap-probe` (§6a) |
| UPnP tests (`TestSelectBestService`, `TestGetUPnPClient`, …), `TestGetUPnPErrorsMetric` | Phase 2 |

### 3.2 Plan cases and acceptance gates
Every case in plan §8.2 that is tagged for phase 0 or 1 is referenced by a test docstring: 1–33, 20a/20b, 24a,
L1, L2, P1p and M1–M17. Case 34 is the fuzz crate.

**Phase-1 acceptance gates (§8.4)**: cases **7, 9, 12, 16, 18, 20, 23, 24, 27, 30** and **M1–M17**
all pass. Where the main ones live:
- `ts_portmapper/src/client_tests.rs` (37 tests): lifecycle cases 5–25 and M1–M15. This
  includes the phase-1 review regressions: same-generation probe vs renewal, rollback during an
  attempt, resume aging, cancelled cleanup, bounded datagrams against a silent router, and
  warn-once bounds.
- `ts_portmapper/src/{oneshot,probe_tool,cap,testing/tests}.rs`: 20a/20b, 21, 24a, P1p, M7, M9,
  M10 and M17.
- `ts_runtime/src/portmap.rs` (14 tests): case 30 end to end through `DirectManager` (type-3
  endpoint advertised, delete seen within the shutdown timeout), case 31, M16, the driver retry,
  the `only-tcp-443` wake and the release guard.
- `ts_runtime/src/direct.rs`: cases 26, 28 and 29. `ts_magicsock/src/sock.rs`: case 26 and M6.
  `ts_control/src/tokio/client.rs`: case 27.
- `checks`: case 24 (dependency and feature graph, tripwire fixtures).

Additional cases beyond the plan come from the reviews: PCP short-error spoofing, the NAT-PMP
address error vs a MAP refusal, kernel-route mismatch in source-IP mode, interrupts in the probe,
`--portmap-wait`, short-lease renewal during hold, implausible external addresses, ipv6_route
overflow (450 veths), warn-once key bounds, sleep aging, and the probe backoff.

### 3.3 Results of this verification (macOS 26 / Darwin 25.6, `cargo +1.95.0`, `-j4`, `RUST_TEST_THREADS=4`)
| Check | Result |
|---|---|
| `cargo test --locked --workspace` (at `fd527db`) | **Pass**: 2497 passed, 0 failed, 1 ignored (pre-existing `open_real_tun_device_requires_root`), 121 test binaries |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (at `5077267`) | **Pass** |
| `cargo fmt --all -- --check` (at `5077267`) | **Pass** (only the usual nightly-only rustfmt option warnings) |
| `cargo run -q -p checks` (at `5077267`) | **Pass**: all 12 checks, including `portmapper_lan_only` and `portmapper_test_isolation` |
| `ts_portmapper` after the fixes | 173 unit + 3 doc tests pass. The full suite was run 12 times, 4 copies at once, with no failures. The fixed test passed 100 of 100 runs. |
| `ts_runtime` port-mapper tests under load | 14 tests × 8 runs (4 at once): no failures |
| `exit_node` build (`exitnode` profile, `ring-aead`) | **Pass** |
| Fuzz (`ts_portmapper/fuzz`, nightly; built by hand with sancov, `--cfg fuzzing`, debug assertions and overflow checks, no ASan, because `cargo-fuzz` isn't installed here) | **Pass**: each target ran 5 min clean. `parse_pcp` 225 M runs, `parse_pcp_map` 220 M, `parse_pmp` 213 M, `parse_rt_dump` 194 M, `parse_proc_net_route` 80 M. |

The workspace test run happened before the three verification commits. Those commits change only
test code and the fuzz lock in `ts_portmapper`, which was retested in full.

## 4. No test contacts the real network
- **Static:**
  - `FakeRouter::start` refuses a non-loopback bind; `TestCap` asserts its router is on loopback;
    `send_datagrams` refuses a non-loopback destination.
  - Logical gateways such as `192.168.1.1` exist only as `TestCaps` keys mapped to loopback ports.
  - In `ts_runtime`'s test build, the production lookup is a stub that panics. Every shared test
    `ForwarderConfig` sets `port_mapper: None`.
  - `tests/*.rs` that build a `Device` or `Server` set `TS_DISABLE_PORTMAPPER=1`. The
    `portmapper_test_isolation` check enforces this.
  - The facade's `Server::new()` unit tests never start a device.
  - The only real-host calls are local reads: the routing-table sysctl, the interface scan, a
    never-sending `connect()` for source selection, and `netstat -rn`. The real-gateway entry
    points (`Config::system`, `probe_tool::run`) are called only by the runtime with the mapper
    enabled and by the `--portmap-probe` CLI path. No test reaches them.
  - No multicast or SSDP code exists.
- **Dynamic:** I ran the whole workspace test suite under
  `sandbox-exec` with `(deny network-outbound (with send-signal SIGKILL))`, allowing only
  `localhost` and unix sockets. **Any** non-loopback `connect` or `sendto` kills the test binary;
  I checked this with a probe that sends to `192.0.2.1:5351`.
  - Every port-mapper test passed: all of `ts_portmapper`, all 14 `ts_runtime::portmap` tests,
    the direct-manager, control and magicsock port-mapper tests, the checks, and the `tests/`
    integration tests.
  - 14 tests were killed. All of them already exist on `origin/main` and are unrelated to the
    mapper. They send disco or relay traffic, or dial, to documentation or fake peer addresses:
    - 3 in `ts_runtime::multiderp`;
    - 8 in `ts_magicsock::sock`;
    - `ts_forwarder::dialer::tests::host_exit_dialer_does_not_refuse_public_exit_udp`;
    - 2 in `ts_forwarder/tests/ipv4_only_guard.rs`.

    With those skipped, every remaining test passes under the sandbox.

## 5. Known gaps
1. **Field validation not done.** This covers the phase-0 field gate (the probe in kmini's launch
   context, inbound reachability, deletion), the phase-1 field check and the overnight
   renewal/no-storm observation. §6 gives the procedures. Per plan §6, the phase-0 field gate
   should pass before phase 1 is deployed.
2. **Not deployable as-is.** The branch is based on `origin/main`. Its binary lacks `--serve-tcp`
   and `--interactive`, which `deploy/launchd/run.sh` uses, so `update.sh` refuses it. An
   integration branch is needed (§6b). A read-only `git merge-tree ts-exit/main feat/portmapper`
   reports 5 conflicting files:
   - 4 conflicts already exist between `ts-exit/main` and `origin/main`, because the control
     watchdog landed upstream as #634 in a different form: `CHANGELOG.md` and
     `ts_control/src/tokio/{client.rs,map_stream.rs,map_watchdog.rs}`.
   - 1 comes from this branch: `examples/exit_node/main.rs`, where the port-forwarding flags and
     the port-mapper flags both add arguments.
3. **Linux was not exercised in this verification** (macOS only). The Linux parser and the
   `SO_BINDTODEVICE` path are unit-tested through fixtures and cfg; the Linux-only
   `127.0.0.2:5351` harness variant (§8.3) was not built.
4. **Fuzzing:** before `5077267` the fuzz crate could not build `--locked`, so earlier fuzz runs (if
   any) predate the lifecycle client's `tracing` dependency. This verification ran each target for
   5 min, clean (§3.3), without AddressSanitizer. The parsers have no `unsafe`, so ASan adds
   little; a `cargo fuzz run` with the default ASan build is still a worthwhile repeat once
   `cargo-fuzz` is installed.
5. **Logging of unchanged renewals is at DEBUG**, which release builds strip. The overnight check
   therefore has to use the status line (`portmap=active-…` with the same `ext=`), the absence of
   `mapping lost` / `expired` lines, and the router's lease countdown, not a "renewed" line.
6. **A renewal that timed out against a router that had just rebooted and committed it** is not
   kept as a deletable orphan once a later probe sees the epoch roll back. The rule is "left to
   expire" (bounded by its granted lifetime, ≤ 7200 s requested). Commit `447e6e4` handles only
   the case where the attempt is still in flight. This is a narrow corner; I found it while
   diagnosing the flaky test. Consider a follow-up.
7. **Deferred by design:** UPnP-IGD and SSDP (phase 2), epoch checks on every reply, a listener for
   PMP multicast announcements, persisted ownership across restarts, an OS link monitor (the
   10 s poll stands in), and IPv6 mappings. Linux reads the main routing table only; policy-routed
   hosts need `--portmap-gateway`.
8. **Exposure** (plan §9.7): a port-mapped endpoint reveals the home public IP to tailnet peers,
   like STUN. `exit_proxy` deployments should consider `--no-portmapper`.

## 6. Runbooks

Port choices. kmini's Rust node uses `--port 41642`, and the Go `tailscaled` on the same Mac uses
41644. The probe refuses 41641, 41642 and `--port`. The runbooks below use test port **41700**.
Output and logs may contain the home public IP and the laptop's address. **Never commit them;**
redact them in any `docs/` capture.

### 6a. `--portmap-probe` once, in kmini's launchd context (a separate one-shot job)
Don't change `dev.kasper.rust-exit` or its plist; it has `KeepAlive=true` and would rerun the
probe forever. Instead, use a separate system job with the same user, the same `/bin/zsh`
wrapper style and the same domain. Run these on kmini as yourself; only the launchctl steps need
sudo.

```zsh
app="$HOME/Library/Application Support/rust-exit"
label=dev.kasper.rust-exit-portmap-probe
# 1. The probe binary sits next to the live one on the internal disk (the live
#    bin/exit_node is not replaced; update.sh's pkill pattern does not match this name).
install -m 755 /Volumes/git/ts-exit/bin/exit_node-portmap "$app/bin/exit_node-portmap"
# 2. A wrapper like run.sh. Each protocol is mapped separately; the hold (240 s) is shorter
#    than the requested lease (300 s); --portmap-wait covers a route that is not up yet.
cat > "$app/portmap-probe.sh" <<'EOF'
#!/bin/zsh
# One-shot NAT-PMP/PCP probe, started by launchd (dev.kasper.rust-exit-portmap-probe).
set -u
dir=${0:A:h}
exec $dir/bin/exit_node-portmap --portmap-probe --portmap-proto both \
  --portmap-test-port 41700 --portmap-lifetime 300 --portmap-hold 240 --portmap-wait 60
EOF
chmod 755 "$app/portmap-probe.sh"
# 3. A one-shot system daemon: same UserName, GroupName, HOME and Umask as the node; KeepAlive false.
tmp=$(mktemp)
cat > $tmp <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key><string>$label</string>
	<key>UserName</key><string>$USER</string>
	<key>GroupName</key><string>staff</string>
	<key>ProgramArguments</key>
	<array><string>/bin/zsh</string><string>$app/portmap-probe.sh</string></array>
	<key>EnvironmentVariables</key><dict><key>HOME</key><string>$HOME</string></dict>
	<key>WorkingDirectory</key><string>$app</string>
	<key>RunAtLoad</key><true/>
	<key>KeepAlive</key><false/>
	<key>ProcessType</key><string>Interactive</string>
	<key>Umask</key><integer>63</integer>
	<key>StandardOutPath</key><string>$HOME/Library/Logs/rust-exit/portmap-probe.log</string>
	<key>StandardErrorPath</key><string>$HOME/Library/Logs/rust-exit/portmap-probe.log</string>
</dict>
</plist>
EOF
plutil -lint $tmp
sudo install -m 644 -o root -g wheel $tmp /Library/LaunchDaemons/$label.plist; rm -f $tmp
# 4. Run it (RunAtLoad starts it once) and watch it.
sudo launchctl bootstrap system /Library/LaunchDaemons/$label.plist
tail -f ~/Library/Logs/rust-exit/portmap-probe.log
```

**During each 240 s hold**, test reachability from **outside** the home network: the hard-NAT
laptop at the café, or a phone hotspot. The target is the `external=<ip:port>` from that
protocol's `map` line:
```zsh
for i in 1 2 3; do echo probe | nc -u -w1 <external-ip> <external-port>; sleep 1; done
```
The log should show `inbound proto=… n=… src=… len=6`. Also check the router's NAT-PMP port-forward
list in the Asus UI: it should show a UDP rule to kmini's LAN IP on port 41700 while the hold
runs, and none after the delete. Go's 41644 rule stays.

**Pass** if the following hold (either protocol is enough, per plan §6):
- `env … uid=euid=<you> ppid=1 xpc_service=dev.kasper.rust-exit-portmap-probe`;
- `gateway … bind=ifscope`;
- `probe pmp=true …` and/or `pcp=true`;
- `map proto=… result=ok external=<ip>:<nonzero port>`;
- `inbound` lines arrive from the outside client;
- `delete proto=… sent=ok reply=ok lifetime=0`;
- `launchctl print system/$label | grep 'last exit code'` shows `0` (both protocols) or `2`
  (some).

`EPERM` or `EHOSTUNREACH` is printed verbatim. Either one would indicate Local Network privacy or
routing trouble in the daemon context; record it and stop.

Optional variants: kickstart the job again with
`sudo launchctl kickstart system/$label`. This appends to the same log.
- Right after a sleep/wake cycle.
- **Before-login boot:** leave the plist installed across a reboot and read the log after
  logging in. `gateway wait=…s attempts=…` shows whether the route was late.
- Single protocol: edit `--portmap-proto pmp` or `pcp` in the wrapper.

**Clean up** afterwards:
```zsh
sudo launchctl bootout system/$label 2>/dev/null; sudo rm -f /Library/LaunchDaemons/$label.plist
rm -f "$app/portmap-probe.sh" "$app/bin/exit_node-portmap"
```

### 6b. Deploy with `deploy/launchd/update.sh`
1. **Build an integration binary** (prerequisite; `bin/exit_node-portmap` itself is refused, as
   §5.2 explains). In `repos/geiserx-tailscale-rs`, create a local branch from `ts-exit/main`,
   merge `feat/portmapper` and resolve the 5 conflicts:
   - the watchdog files: take the `ts-exit/main` version that kmini runs today, unless you rebase
     the fork onto upstream #634 first;
   - `exit_node/main.rs`: keep both flag sets.

   Run the gates and build:
   ```zsh
   cd /Volumes/git/ts-exit/repos/geiserx-tailscale-rs
   git switch -c ts-exit/portmapper ts-exit/main && git merge feat/portmapper   # resolve, commit
   cargo +1.95.0 test --locked --workspace && cargo +1.95.0 clippy --locked --workspace --all-targets -- -D warnings \
     && cargo +1.95.0 fmt --all -- --check && cargo +1.95.0 run -q -p checks
   cargo +1.95.0 build --locked --profile exitnode --features ring-aead --example exit_node
   cp target/exitnode/examples/exit_node /Volumes/git/ts-exit/bin/exit_node-portmap-int
   /Volumes/git/ts-exit/bin/exit_node-portmap-int --help | grep -E -- '--serve-tcp|--interactive|--no-portmapper'
   ```
2. **Keep the current release** for rollback. `update.sh` overwrites `*.prev` on every run.
   ```zsh
   cp -p "$HOME/Library/Application Support/rust-exit/bin/exit_node" /Volumes/git/ts-exit/bin/exit_node-pre-portmap
   ```
3. **Deploy locally on kmini**, not over `ssh kmini`, because the restart drops that session.
   `run.sh` needs no change: the runner turns the mapper on with its defaults (suppress
   non-public addresses, renew while idle with peers).
   ```zsh
   /Volumes/git/ts-exit/deploy/launchd/update.sh /Volumes/git/ts-exit/bin/exit_node-portmap-int
   ```
   `update.sh` restarts the node and requires within 90 s the `running` log line, an HTTPS fetch
   through the exit node, and tailnet SSH. Otherwise it rolls back by itself.
4. **Confirm on kmini:**
   ```zsh
   sed -E 's/\x1b\[[0-9;]*m//g' ~/Library/Logs/rust-exit/exit_node.log | grep -E 'portmapper:|portmap=' | tail -20
   ```
   Expect:
   - `portmapper: gateway found`;
   - `probe result changed pmp=Some(true) pcp=Some(true)`;
   - `portmapper: mapping obtained proto=pmp external=<ip:port> lease=…`;
   - within one status interval (300 s), a `self` line with `portmap=active-_MC ext=<ip:port>`.
     `_` means UPnP was not checked; `M` and `C` mean PMP and PCP are present.

   The router should list a second NAT-PMP rule, to port 41642, next to Go's 41644 rule.

### 6c. Verify from a hard-NAT client
On the laptop, on the hard-NAT network (café):
```zsh
tailscale netcheck | grep -E 'MappingVariesByDestIP|PortMapping'   # expect MappingVariesByDestIP: true
tailscale ping -c 10 kmini          # expect "pong from kmini (100.112.127.25) via <public-ip>:<mapped port>"
tailscale status | grep kmini       # expect "direct <public-ip>:<mapped port>", not relay "fra"
tailscale status --json | jq '.Peer[] | select(.HostName=="kmini") | {CurAddr, Relay, Addrs}'
```
- The first pings may go via DERP. The path should turn direct within about 1 min.
- `Addrs` should list the port-mapped endpoint first.
- Then use kmini as the exit node (`tailscale set --exit-node=kmini`; fetch a page), and check
  that kmini's own log shows the laptop's peer line as `path=direct …`.
- For comparison, before the deploy the same laptop stayed on the Frankfurt relay.

**Overnight:**
- the `ext=` port stays stable across the hourly renewals;
- no `mapping lost`, `expired` or repeated `portmapper:` WARN lines;
- the router's lease for 41642 keeps resetting.

  Because of gap §5.5, use the router lease countdown, not a log line.

### 6d. Turning it off
- **Keep the build, mapper off:** add `--no-portmapper` to the `exit_node` command in
  `deploy/launchd/run.sh`, or add `export TS_DISABLE_PORTMAPPER=1` there. Then reinstall:
  ```zsh
  /Volumes/git/ts-exit/deploy/launchd/update.sh /Volumes/git/ts-exit/bin/exit_node-portmap-int
  ```
  The restart's graceful shutdown deletes the router rule. The new process then constructs no
  client and sends nothing, and the facets stay unset. Revert the `run.sh` edit before running
  any pre-portmap binary, because older binaries reject `--no-portmapper`. `update.sh`'s health
  check would catch that and roll back anyway.
- **Roll back the binary:**
  `/Volumes/git/ts-exit/deploy/launchd/update.sh /Volumes/git/ts-exit/bin/exit_node-pre-portmap`.
  The outgoing portmap build releases its rule on shutdown.
- **Check** that the router's list no longer shows the 41642 rule. If a delete was lost, the rule
  lasts until its granted lifetime ends (7200 s requested; MiniUPnPd may cap it). The probe's
  rules are 300 s.
- There is no live toggle without a restart. Control's `only-tcp-443` stops all mapper I/O but
  also changes the node's UDP behaviour, so don't use it as an off switch.
