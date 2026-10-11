# Known limitations and security caveats

State of fork branches `ts-exit/runner` and `ts-exit/perf-connected` (github.com/kasper0406/tailscale-rs). Experimental and unaudited. Run on a restricted tailnet with your own
devices only, and don't route sensitive traffic through it.

## Security

| # | Limitation | Impact | Mitigation now | Fix |
|---|---|---|---|---|
| S1 | Control updates (peers, packet filter, routes) reach the dataplane over a best-effort message bus (`kameo` `BestEffort`, mailbox 64, no replay). | A dropped update can leave a **revoked permission or peer in force** until a later full update. | Restricted tailnet; restart the node after ACL changes you depend on. | Reliable ordered delivery plus generation-tagged snapshots (codex-02-ipv6.md §4; 1–3 days). |
| S2 | ~~Node-key replacement keeps the old WireGuard session~~ **Fixed** (`6bfbbee`, `681cd75`, by Codex): a key/PSK change recreates the tunnel peer (sessions, pending handshakes, timers, MAC state); expired peers are removed from the tunnel and from installed routes. | Still depends on the expiry/key update actually being delivered (S1). | — | — |
| S3 | Existing flows aren't re-checked after an ACL change (non-SYN TCP passes; UDP reverse cache has no TTL). | Same as Go for TCP. UDP is slightly looser. | — | Document; flush the UDP cache on filter change. |
| S4 | SYN-flood memory: listeners keep up to 128 half-open sockets each, with buffers allocated eagerly (virtual; RSS grows only when used). | An ACL-authorized peer can inflate the forwarder. | Only your own devices are authorized. | Budget sockets before allocation; small handshake buffers. |
| S5 | Exit egress refuses loopback, RFC 1918, link-local, CGNAT, multicast, broadcast, 0/8 and 240/4. | Stricter than Go, which allows the exit host's own interface IPs. | — | Intended. |
| S6 | Unaudited crypto (fork's own WireGuard implementation, `ts_tunnel`). | — | Test traffic only. | Audit. |

## Functional

| # | Limitation |
|---|---|
| F1 | IPv4 egress only. `::/0` is advertised (official clients require the pair), but IPv6 is answered with TCP RST / ICMPv6 prohibited and never forwarded. This home network has no IPv6 anyway. |
| F2 | No exit-node DNS (peerAPI DoH) service: `Config` hardcodes `peerapi_port: None`. Clients fall back to their configured/public resolvers, whose queries go through the exit node as ordinary UDP/TCP 53. To be verified on the iPad. |
| F3 | No ICMP forwarding (ping through the exit node fails) and no PMTU/ICMP error relay. Fragmented first datagrams to a cold UDP port get port-unreachable. |
| F4 | TCP: the overlay handshake completes before the upstream dial (Go dials first), so a client sees "connected" and then a close if the upstream is unreachable. Failures close gracefully instead of with RST. |
| F5 | Single-flow throughput ≈ `tcp_buffer_size / RTT` (no autotuning, 256 KiB default). smoltcp has no SACK, so it handles loss and reordering worse than kernel TCP. |
| F6 | Network changes: rebind exists but the receive loop can stay parked on the old socket until traffic arrives (codex-audit-01 "Rebind receive stall"). The `network-monitor` feature isn't enabled in the runner. |
| F7 | Underlay: by default one unconnected UDP socket, per-packet syscalls and allocations (BENCHMARKS §1b). `ts-exit/perf-connected` adds opt-in connected sockets + DF (`TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1`, `TS_DEBUG_DONT_FRAGMENT=1`); no batching (`sendmsg_x`) yet. |
| F8 | The runner doesn't daemonize under launchd. `start.sh` uses `nohup`, so it doesn't survive reboot. Intentional for experiments. |
