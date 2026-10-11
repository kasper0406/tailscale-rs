# RUNBOOK: Rust exit node on the Mac mini

Experimental, unaudited networking code (see "Known limitations" below and `docs/LIMITATIONS.md`).
Use it on a restricted tailnet, for test traffic only. The ASUS router VPN stays the fallback for remote access.

Everything here is userspace. The node needs no root, never changes the Mac's routes, DNS or `pf`, and
keeps all state in `/Volumes/git/ts-exit/state/`. Stopping the process is the full rollback on the Mac.

## 0. One-time tailnet setup (admin console)

1. **Tag and ACL policy** (Access controls). The tag lets you aim policy at this node only:

   ```jsonc
   "tagOwners": { "tag:rust-exit": ["autogroup:admin"] },
   "grants": [
     // Who may use exit nodes. Keep it to your own user (the iPad and test laptop log in as you).
     { "src": ["<you>@<domain>"], "dst": ["autogroup:internet"], "ip": ["*"] }
     // Add nothing with "src": ["tag:rust-exit"]: the exit node needs no access to your devices.
   ],
   // Optional: approve the exit route automatically instead of by hand (step 4).
   "autoApprovers": { "exitNode": ["tag:rust-exit"] }
   ```

   `autogroup:internet` is what limits exit traffic to the public internet in the policy control sends.
   The node also refuses loopback, RFC 1918, link-local, CGNAT (tailnet) and multicast destinations
   on its own, so it never reaches the home LAN or other tailnet devices even under a broader policy.

2. **Auth key** (Settings → Keys → Generate auth key): **reusable**, expiry **1–3 days**, **tags: `tag:rust-exit`**
   (pre-approved too, if device approval is on). Then, on the Mac, without the key touching shell history:

   ```sh
   mkdir -p ~/.config/ts-exit && chmod 700 ~/.config/ts-exit
   pbpaste > ~/.config/ts-exit/authkey && chmod 600 ~/.config/ts-exit/authkey   # key copied to clipboard
   ```

   The key is read only on first start (when `state/rust-exit/keys.json` doesn't exist yet). It is never logged
   or passed on the command line.

## 1. Build

```sh
/Volumes/git/ts-exit/runner/build.sh
```

That runs `cargo build --release --locked --example exit_node` in the fork checkout (branch `ts-exit/runner`),
for `aarch64-apple-darwin`.

## 2. Start / status / stop

```sh
runner/start.sh                      # exit node: advertises 0.0.0.0/0 + ::/0, forwards all ports
runner/start.sh --no-advertise       # tunnel-only tests: joins the tailnet, advertises nothing
runner/status.sh                     # pid, CPU/RSS, last self/peer lines (direct vs DERP)
runner/stop.sh                       # SIGTERM, SIGKILL after 10 s
```

Log: `state/rust-exit/exit_node.log`. Every 30 s it prints one `self` line (advertised routes, health) and one
`peer` line per online peer, with `path=direct <ip:port>` or `path=derp <region>`.
**Quote the path with every measurement**: results from a DERP-relayed run aren't comparable to direct ones.

Settings (`runner/env.sh`, overridable via environment): hostname `rust-exit`, tag `tag:rust-exit`,
WireGuard UDP port 41642 (not the official client's 41641), state `state/rust-exit`.
Extra runner flags pass through `start.sh`, e.g. `--tcp-buffer-size 2097152` (see BENCHMARKS.md).

## 3. Verify it joined

`runner/status.sh` shows `running ... v4=Some(100.x.y.z)`, and the `self` line lists
`routes=[0.0.0.0/0, ::/0]` (plus its own /32 and /128). In the admin console the machine `rust-exit` appears
with the tag, and "Exit node" shows as awaiting approval (or approved, with autoApprovers).

## 4. Approve the exit route

Admin console → Machines → `rust-exit` → ⋯ → Edit route settings → enable **Use as exit node**.
Skip this step if autoApprovers is set. Within a few seconds the runner's `self` line shows the route as accepted.

## 5. Use it from the iPad (official Tailscale app)

1. Tailscale app → **Exit Node** → choose `rust-exit`. (It only appears when both 0.0.0.0/0 and ::/0 are
   advertised **and** approved.) Leave "Allow LAN access" off.
2. Safari → `https://ifconfig.me`: it must show the **home public IPv4** (recorded locally in `docs/live-results.md`),
   not the cellular carrier's address. Test from cellular, not home Wi-Fi, so the path is real.
3. A few sites, DNS lookups (new domains), and a video (F1 TV / YouTube). IPv6-only sites will fail: the node
   rejects IPv6 on purpose and dual-stack sites should fall back to IPv4 immediately.
4. On the Mac, `runner/status.sh` shows the iPad as a peer with `path=direct ...` (expected) or `path=derp ...`.

To stop using it: Exit Node → **None**.

## 6. Test laptop (official client, outside the home network)

```sh
tailscale ping rust-exit                   # "pong from rust-exit (100.x) via <ip:port>" = direct; "via DERP(xyz)" = relayed
tailscale set --exit-node=rust-exit
curl -4 https://ifconfig.me                # home public IPv4
curl -6 --max-time 5 https://ifconfig.me   # must fail fast (connection refused/unreachable), not hang
tailscale set --exit-node=                 # stop using it
```

Benchmarks: see BENCHMARKS.md for the exact commands.

## 7. Diagnostics

| Symptom | Check |
|---|---|
| Node missing from the iPad's exit list | `self` line routes include both `0.0.0.0/0` and `::/0`; route approved in admin console; your grant includes `autogroup:internet` |
| Exit selected, nothing loads | `runner/status.sh`: is the iPad listed as a peer? `RUST_LOG=info,ts_forwarder=debug runner/start.sh` and look for `dial refused`/`forbidden` |
| Slow first load of each site | Should not happen any more (port gate); if it does, look for `port gate: at max active ports` |
| Single download slow at high RTT | Window-limited by `tcp_buffer_size` (256 KiB default) → try `--tcp-buffer-size 2097152` |
| `path=derp` only | UDP 41642 blocked/NAT; direct is still preferred when possible, and DERP works but is slower |
| Hangs on start | Auth key invalid/expired (`registration` errors in log); delete `state/rust-exit/keys.json` and use a fresh key |

More logging: `RUST_LOG=debug runner/start.sh` (verbose; contains peer IPs, never the auth key).

## 8. Rollback

1. iPad / laptop: Exit Node → None.
2. Mac: `runner/stop.sh`. Nothing else on the Mac was changed.
3. Optional full removal: admin console → Machines → `rust-exit` → Remove; Settings → Keys → revoke the key;
   `rm -r /Volumes/git/ts-exit/state/rust-exit ~/.config/ts-exit/authkey`.

## Known limitations (summary; details in docs/LIMITATIONS.md)

- IPv4 only. IPv6 is advertised (clients need the pair to offer the node) but always rejected.
- Policy updates from control go over a best-effort internal bus. A dropped update could leave a revoked
  permission in force until the next full update. Don't rely on revocation being instant.
- A peer whose node key changes keeps its old WireGuard session for up to ~3 minutes.
- Single-flow throughput is capped by the fixed TCP window (no autotuning). See BENCHMARKS.md.
- Not audited. Don't route sensitive traffic through it.
