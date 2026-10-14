# Brief 27: node stays on DERP after a peer changes networks; dropped half-open sockets

Repo `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`, branch `ts-exit/main` (39973cd; layered on GeiserX/tailscale-rs
main). The live exit node runs `ts-exit/live-candidate`-equivalent code (it already includes your brief-25 fix,
`magicsock: keep probing after a candidate send fails`, now merged upstream as GeiserX #606) with
`TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1 TS_DEBUG_DONT_FRAGMENT=1 TS_DEBUG_MAGICSOCK_BATCHED_IO=1` on macOS
(see `RUNBOOK.md`, `docs/DECISIONS.md` F31–F35, `docs/RESTART-DIRECT-25.md`).

## Problem 1: stuck on DERP after the peer moves (live, 2026-10-14)

- Node up since 2026-10-13 23:52 UTC. Laptop client (official Go Tailscale macOS app, behind NAT, away from home)
  was `path=direct` all night.
- ~04:54 UTC the laptop apparently changed networks (it slept/moved; its access link went from ~200 to ~50
  Mbit/s). The node's path flapped direct/DERP until 05:04, then has been `path=derp fra` continuously for 3+ hours.
- The laptop's own `tailscale status` says **direct** to the node (its disco pings reach the node and the node
  pongs). Through the exit, everything is capped at ~21 Mbit/s with 855 ms loaded latency (DERP).
- No node restart was involved, so this is not the brief-25 bug. `MappingVariesByDestIP` and the laptop's
  endpoints are being collected from the laptop; Claude will append them below when available.

Go recovers from this within seconds: `handlePingLocked` learns the ping's source (`addCandidateEndpoint`) and the
next `sendDiscoPingsLocked` probes it; a matching pong from that address makes it the best path even when the
peer's netmap endpoints are unreachable (port-dependent NAT). Our `handle_disco` also calls
`add_peer_endpoints(sender, [from])` on an authenticated ping (`ts_magicsock/src/sock.rs` ~2900), yet the node
never confirms a direct path.

Investigate, with evidence:
1. Does the learned ping source survive? Check whether netmap reconcile (`ts_runtime/src/direct.rs`,
   `add_netmap_candidates` / candidate replacement in `ts_magicsock/src/path.rs`) or pruning drops learned
   candidates, whether they are filtered, and whether `candidates_to_ping` ever selects them.
2. Our perf layer's **connected UDP sockets** (`ts_magicsock/src/connected.rs`, enabled live on Apple) and
   batched I/O: when the peer's endpoint changes, do pings/pongs to the new source go out on the right socket,
   does a stale connected socket win, and is the trusted best address ever updated? Compare behaviour with the flag
   off. This is our code, not GeiserX's; say clearly which layer the bug is in.
3. What happens to trust/best address when the old endpoint dies (flap 04:54–05:04), and could the path state get
   stuck (e.g. best kept but untrusted, never re-pinged; or pings sent only to the dead address)?
4. Reproduce deterministically in a test (simulated peer whose external address/port changes and whose NAT only
   accepts replies to the address it sent from), failing before the fix. Fix to match Go. Put the fix on a new
   branch cut from the layer the bug belongs to (`ts-exit/fixes` if it's GeiserX code, `ts-exit/perf` if it's the
   connected-sockets code), in a worktree under `/Volumes/git/ts-exit/scratch/`.
5. Release builds compile out debug logs (`release_max_level_info`). Provide a minimal way to build the exitnode
   binary with magicsock/direct debug logs enabled for a live diagnostic run, and list the log lines Claude should
   look for.

## Problem 2: "half-open socket in unexpected state, dropping"

The forwarder's listener Accept path (`ts_netstack_smoltcp_core/src/socket_impl/tcp/listener.rs`, the `_` arm in
the half-open `retain`) logs this without the state. Live it fires in bursts (12, 10, 21 times), sometimes when
clients' HTTPS connections through the node fail with `errSSLClosedNoNotify` (networkQuality upload, intermittent).
Determine which states reach that arm (FinWait1/2, Closing, TimeWait, LastAck, Closed, SynSent?), whether dropping
them from the queue leaks the socket or drops a connection that should have been accepted (e.g. an established
connection whose peer already sent data+FIN, or one that the TCP recovery/pacing changes now move through states
faster), and fix it: include the state in the log and handle each state correctly. Test it.

## Constraints

- Don't touch the live node or its state dir, don't join the tailnet, don't push or touch remotes.
- Commits: `Signed-off-by: Kasper Nielsen <kasper0406@gmail.com>` and `Co-Authored-By: GPT-6.1 (codex)
  <noreply@openai.com>`. No public IP addresses anywhere (203.0.113.0/24 in tests).
- Full tests of touched crates, clippy `-D warnings`, fmt. `nice -n 10`, at most 4 build jobs.

## Deliverable

`/Volumes/git/ts-exit/docs/DERP-STUCK-27.md`: root cause(s) with code references, tests, fixes and branches, whether
GeiserX `origin/main` and official tailscale-rs have the same bug, and the diagnostic-build instructions. Concise.

## Live data (appended by Claude)
- Laptop `tailscale netcheck` (2026-10-14 07:37 UTC): UDP true; IPv4 yes (public, one mapped port);
  **MappingVariesByDestIP: false** (easy NAT, no port-dependent mapping); IPv6 no; no PortMapping (no UPnP/PMP/PCP);
  nearest DERP London 7.6 ms. So the node's pings to the laptop's netmap endpoint should succeed; hard NAT is not
  the explanation.
- Laptop `status --json` for rust-exit: CurAddr = node's public ipv4:41642 (laptop thinks direct), Relay fra.
- Timeline: the node went to DERP at 05:04 UTC while the laptop was **asleep** (maintenance dark-wake cycles with
  TCPKeepAlive); the laptop woke at 07:10:25 UTC on a **different Wi-Fi LAN** (new DHCP lease, different netmask).
  So: path lost during sleep, then never regained after the peer came back on new endpoints. The node was never
  restarted.
