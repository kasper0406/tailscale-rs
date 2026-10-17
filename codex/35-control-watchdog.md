# Brief 35: the node's control connection died silently; add Go's map-stream watchdog

## What happened (live, 2026-10-16/17)

The live exit node `kmini` (launchd daemon; build `ts-exit/main` = `ts-exit/tcp-0.14` 92c7b2e, i.e. GeiserX
main 18fc6bc + serve flag + patched smoltcp; repo `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`) kept running
and forwarding for peers it already knew, but **Tailscale's control server marked it offline from 00:09 UTC**
("offline, last seen 6h ago" in another client's `tailscale status`). Clients then showed the exit node as
unavailable ("blocked"), and new netmap changes stopped arriving (the laptop dropped out of the node's peer list).
The node logged **nothing** about control at 00:09 or afterwards. Its 5-minute status lines kept reporting
`health=[]`. A process restart fixed it immediately ("netmap stream started", control shows it online). Log:
`~/Library/Logs/rust-exit/exit_node.log` (read-only; around 2026-10-17T00:00–00:15 UTC).

Most likely the long-poll map stream (Noise/HTTP2 to controlplane.tailscale.com) became a dead half-open
connection (NAT mapping expiry, Wi-Fi/ISP blip) and the client waits forever with no read timeout. Go's
controlclient protects against this: the server sends `KeepAlive` map responses (~every minute), and the client's
watchdog restarts the map poll if nothing arrives within 120 s ("map response long-poll timed out!"), in
`control/controlclient/direct.go` (the local Go checkout is `/Volumes/git/ts-exit/repos/tailscale-go`). Also check
HTTP/2 PING and TCP keepalive settings on the control connection.

## Do

1. Confirm the root cause in `ts_control` (map stream reading, keepalive handling, reconnect logic, health
   reporting) with code references. Does it consume `KeepAlive` messages, have any read/idle timeout, and retry on
   error? Why was nothing logged, and why does `health` stay empty?
2. Reproduce it in a test: a map stream that stops delivering (no FIN/RST, no keepalives) must lead to a
   reconnect within a bounded time. Simulate it with a fake control transport or a stalled reader.
3. Fix it the way Go does: a watchdog that restarts the map poll after 120 s without any map response (keepalives
   count), with a log line at warn and a health warning while disconnected. Optionally add HTTP/2 PINGs or TCP
   keepalive, if the transport makes that natural.
4. Put the fix on a branch from GeiserX `origin/main` (`fix/control-watchdog`, worktree under
   `/Volumes/git/ts-exit/scratch/`) so it can become a GeiserX PR, and also check whether official tailscale-rs
   (remote `official`, crates under `crates/`) has the same gap.
5. Workspace tests for touched crates, clippy `-D warnings`, fmt.

## Constraints

Don't touch the live node or its daemon, and don't push. Commits: `Signed-off-by: Kasper Nielsen
<kasper0406@gmail.com>`, `Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>`. No public IPs. `nice -n 10`, at
most 4 jobs.

## Deliverable

`/Volumes/git/ts-exit/docs/CONTROL-WATCHDOG-35.md`: root cause with code references, the test, the fix and
branch, whether GeiserX main and official tailscale-rs share the gap. Concise.
