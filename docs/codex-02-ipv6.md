# IPv6 rejection and security-state follow-up

2026-10-11. **Advertise the default-route pair, keep `enable_ipv6=false`, and reject IPv6 at the dedicated forwarder ingress before smoltcp or a dialer sees it.** Merely advertising `::/0` is insufficient: unmatched ports already reject promptly, but existing wildcard TCP/UDP listeners also accept IPv6.

`F/` means fork source at **719aa889**, checked in detached worktree `scratch/wt-codex`; `G/` means `repos/tailscale-go` at **3bb8a906**. Line numbers below are for those baselines, not Claude's changing runner branch. `S/` means the actual dependency `/Users/knielsen/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smoltcp-0.13.1/src/`. No runner-branch files were edited. The lack of global IPv6/default route on the Mac is an input from Claude; no host network configuration was changed.

## 1. Actual IPv6 path, with either `enable_ipv6` value

| Seam | `false` | `true` | Source evidence |
|---|---|---|---|
| Underlay | Bind IPv4 UDP; can still carry encrypted IPv6 IP packets. | Attempt `[::]` UDP, fallback IPv4; IPv6 endpoint candidates enabled. | `F/ts_runtime/src/direct.rs:223,233,244`; `F/ts_magicsock/src/sock.rs:531,581`. This flag selects underlay/address behavior, not an inner-packet firewall. |
| Decrypt | Same family-independent WireGuard path. | Same. | Magicsock receives ciphertext at `F/ts_magicsock/src/sock.rs:1342,1417`; bridge to dataplane at `F/ts_runtime/src/direct.rs:644`; decrypt at `F/ts_dataplane/src/lib.rs:1715`. Session ID identifies the cryptographic peer, `F/ts_tunnel/src/endpoint.rs:1189,1198,664`. DERP likewise transports ciphertext, not an IPv4-only IP payload. |
| Source ownership | Peer IPv6 `/128` is installed if supplied in its accepted routes. | Same. | `Node` retains `AllowedIPs`, falling back to `Addresses`, `F/ts_control/src/node.rs:1146`; `routes_to_install` keeps Tailscale host routes regardless of `accept_routes`, `:494,508`; source table iterates these without an IPv6 gate, `F/ts_runtime/src/src_filter.rs:49,57`. Decrypted packet source lookup must equal its cryptographic peer, `F/ts_dataplane/src/lib.rs:1754,1759`. A forged other-peer IPv6 source fails this lookup. **An address merely appearing in `addresses` is not sufficient if control supplies an `AllowedIPs` set excluding it.** |
| Packet filter | Parses/checks IPv6 despite comments saying it is reachable only when enabled. | Same. | IPv6 branch `F/ts_dataplane/src/lib.rs:874`; actual rule match `:676`; no `enable_ipv6` input. `F/ts_runtime/src/packetfilter.rs:286` applies both families. The flag changes shields-up self-address bookkeeping at `:272,324`, not transit rule parsing. |
| Accepted default route | Baseline never advertises `::/0`, even if enabled. If self `accepted_routes` nonetheless contains it, routes it to forwarder. | Same. | Advertisement strips IPv6, `F/ts_control/src/config.rs:681,690`; inbound split iterates all accepted routes, `F/ts_runtime/src/route_updater.rs:431,432,438`; `/0` is a subnet/transit route, `F/ts_control/src/node.rs:459`. Self-route messages install it in dataplane, `F/ts_runtime/src/dataplane.rs:334`. Before acceptance there is no default inbound route; advertisement alone grants nothing. |
| Forwarder netstack | IPv6 compiled in and AnyIP enabled. | Same. | `F/ts_netstack_smoltcp_core/Cargo.toml:31,32`; `F/ts_runtime/src/forwarder_actor.rs:164`. Unlike application netstack, forwarder does not receive self IPv6 addresses conditional on this flag. |
| Return | Peer IPv6 host route can encrypt and return a rejection over IPv4 UDP or DERP. | Same, also possible IPv6 underlay. | Outbound peer table uses the same `routes_to_install`, `F/ts_runtime/src/route_updater.rs:246,250`; emitted forwarder packets go to dataplane, `F/ts_runtime/src/forwarder_actor.rs:140`; outbound overlay routing/encryption `F/ts_dataplane/src/lib.rs:1667,1674`. No family gate there. |

### What the ACL actually contains

**Do not assume `autogroup:internet` means `::/0`.** The pinned Go implementation's IPv6 internet-access probe uses `2000::` because that autogroup starts with **`2000::/3`**, `G/ipn/ipnlocal/peerapi.go:841`. A public destination such as `2001:4860:4860::8888` is in that range. Default-route advertisement and ACL destination expansion are separate concerns.

Rust receives already expanded IP rules; it neither recognizes nor expands the autogroup. `F/ts_packetfilter_state/src/lib.rs:24,32,50` converts source/destination prefixes without dropping IPv6. Literal `::/0` survives; `*` expands to both defaults, `F/ts_packetfilter_serde/src/ip_range.rs:85,87,94`. Rule matching requires source, protocol and destination, `F/ts_packetfilter/src/rule.rs:40`. **No actual control-produced netmap for this tailnet was supplied or fetched**, so its exact compiled prefixes cannot be established from these checkouts. Inspect a sanitized received ruleset at the eventual tailnet gate; never substitute a synthetic allow-all filter for that verification.

Security qualification: source ownership is mandatory, but the filter deliberately admits TCP non-SYN packets and valid later fragments before ACL matching (`F/ts_dataplane/src/lib.rs:641,556`). Unauthorized *new SYNs* still fail ACLs. The rejection hook must follow the existing source/filter checks; it cannot turn a rejected SYN into a dial or forward a later fragment. Do not claim every packet reaching it necessarily matched an explicit ACL rule.

### smoltcp: AnyIP DOES include IPv6; listener state changes rejection

`S/iface/interface/ipv6.rs:213` checks `has_ip_addr(dst)`. Crucially, **`has_ip_addr` returns true unconditionally when AnyIP is set**, `S/iface/interface/mod.rs:885,888`; reading the IPv6 function alone gives the wrong answer. Verified with packet injection in the isolated worktree.

* **Unmatched SYN:** TCP dispatch sends RST, `S/iface/interface/tcp.rs:43,53`. The all-port observer is IPv4 only (`F/ts_forwarder/src/all_port.rs:109`; `raw_open(true, ...)` selects IPv4), so it does not suppress this IPv6 RST or create a new IPv6 listener.
* **Unmatched UDP:** generates ICMPv6 destination-unreachable/code 4, `S/iface/interface/udp.rs:49,67`. The UDP observer is also IPv4 only, `F/ts_forwarder/src/all_port.rs:184`.
* **Warm or explicit TCP port:** forwarder binds `0.0.0.0:port`, `F/ts_forwarder/src/tcp.rs:47`, but core converts ANY unspecified address to `addr: None`, `F/ts_netstack_smoltcp_core/src/socket_impl/tcp/listener.rs:21`. This wildcard accepts IPv6 too. It emits SYN-ACK and accepts **before** classification/dialing (`F/ts_forwarder/src/tcp.rs:55,107,114`). With baseline IPv4-only routes, IPv6 classification returns `None`; after adding `::/0`, `RouteTable::classify` returns `ExitNode` for IPv6 too (`F/ts_forwarder/src/class.rs:41`). Host dialing subsequently errors, but dropping an accepted overlay stream requests a graceful close (`F/ts_netstack_smoltcp_socket/src/tcp/stream.rs:345`), not Go's pre-accept RST contract.
* **Warm or explicit UDP port:** wildcard conversion likewise removes the address family, `F/ts_netstack_smoltcp_core/src/socket_impl/udp.rs:33`. A bound UDP socket consumes the packet before native ICMP generation. Unclassified destinations or failed backend dials simply `continue`, `F/ts_forwarder/src/udp.rs:131,142`: a silent application timeout. Claude's pinned UDP 443/53 makes this seam especially relevant.

Existing `ipv4_only_guard.rs` tests establish no successful IPv6 host dial, not prompt on-wire rejection. Its comment claiming route classification strips IPv6 (`F/ts_forwarder/tests/ipv4_only_guard.rs:46`) disagrees with `class.rs:41` once a v6 prefix is inserted.

## 2. Minimal implementation design and prototype

Use independent, startup-only `ExitNodeIpv6::{Reject, Off}`. **Default `Reject`**, with `#[serde(default)]`, makes ordinary `advertise_exit_node=true` compatible with Go's paired-route selection (`G/net/tsaddr/tsaddr.go:185`; `G/ipn/ipnlocal/local.go:1680`). `Reject` advertises both defaults, while deliberately providing no IPv6 egress. `Off` omits the v6 default, retaining old behavior and potentially hiding the node from official exit selectors. `enable_ipv6` continues to control application/underlay IPv6; leave it false on this Mac. Do not automatically enable it to satisfy advertisement.

Core advertisement changes, **design diff, not a complete/applicable patch**:

```diff
--- a/ts_control/src/config.rs
+++ b/ts_control/src/config.rs
@@
+/// IPv6 behavior for an IPv4-only exit forwarder, independent of overlay/underlay IPv6.
+#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
+pub enum ExitNodeIpv6 {
+    /// Advertise ::/0 for Go client compatibility and reject IPv6 transit promptly.
+    #[default]
+    Reject,
+    /// Omit ::/0; official clients may not offer this node as an exit.
+    Off,
+}
@@
     pub advertise_exit_node: bool,
+    #[serde(default)]
+    pub exit_node_ipv6: ExitNodeIpv6,
@@
             push_unique(default_v4);
+            if self.exit_node_ipv6 == ExitNodeIpv6::Reject {
+                push_unique(ipnet::Ipv6Net::new_assert(core::net::Ipv6Addr::UNSPECIFIED, 0).into());
+            }
@@
             advertise_exit_node: false,
+            exit_node_ipv6: ExitNodeIpv6::default(),
--- a/src/config.rs
+++ b/src/config.rs
@@
     pub advertise_exit_node: bool,
+    /// Advertise and reject IPv6 exit transit, or omit its default route.
+    pub exit_node_ipv6: ts_control::ExitNodeIpv6,
@@
             advertise_exit_node: value.advertise_exit_node,
+            exit_node_ipv6: value.exit_node_ipv6,
@@
             advertise_exit_node: false,
+            exit_node_ipv6: ts_control::ExitNodeIpv6::default(),
```

Export the enum from `ts_control/src/lib.rs:61` and public `src/lib.rs`. Also thread it into **`AdvertiseState`**, initialized at `F/ts_runtime/src/lib.rs:376`, and both calls to `compose_advertised_routes` at `:1283,1299`. Change that function's signature to accept the policy; append `::/0` only for `exit_node && Reject` (`:1585`). Otherwise a runtime setter removes the startup v6 advertisement! Explicit IPv6 subnet routes remain filtered; no IPv6 subnet forwarding is being enabled. Both route vectors, startup and live, must stay identical. Setters already update local forwarder routes before wire advertisement (`:1308`); preserve that order.

**Rejection hook:** intercept every IPv6 packet in the dedicated forwarder ingress pump, after decrypt/source/filter/accepted-route dispatch but before smoltcp's bounded ingress queue. Emit a reversed-tuple TCP RST (SYN -> RST|ACK; ACK -> RST with its ACK as sequence) or ICMPv6 type 1/code 1 for ordinary UDP. Consume the packet regardless of whether a safe reply can be constructed. This prevents listener readiness, all-port reinjection, proxy choice and host network conditions from affecting rejection. It also avoids allocating an overlay TCP flow, a UDP mapping, or any host socket.

The actual reviewable prototype is **[codex-02-ipv6-reject.patch](codex-02-ipv6-reject.patch)**, based on 719aa889, covering `ts_forwarder`'s rejection helper/tests and the runtime ingress hook. It intentionally omits configuration/advertisement changes above. Its dependency on `etherparse 0.19` is already present elsewhere in the lockfile; the lock diff only adds the forwarder dependency edge, with no version changes.

Key runtime change, abbreviated from the checked prototype:

```diff
--- a/ts_runtime/src/forwarder_actor.rs
+++ b/ts_runtime/src/forwarder_actor.rs
@@
+        let reject_up = netstack_up.clone(); // before netstack_up moves into downlink task
@@
-            crate::netstack_actor::pump_ingress(netstack_down, netstack_down_tx, "forwarder").await;
+            let mut netstack_down = netstack_down;
+            while let Some(bufs) = netstack_down.recv().await {
+                for buf in bufs {
+                    if buf.as_ref().first().is_some_and(|b| b >> 4 == 6) {
+                        if let Some(reply) = ts_forwarder::reject_ipv6(buf.as_ref()) {
+                            if reject_up.send(vec![reply.into()]).is_err() { return; }
+                        }
+                        continue; // malformed/unsupported IPv6 is consumed too
+                    }
+                    netstack_down_tx.try_send(buf.as_ref()); // retain bounded v4 ingress/drop accounting
+                }
+            }
```

Reject all IPv6 delivered to this IPv4-only forwarder even under `Off`, including stale approved routes; the enum gates *advertisement*, not permission to leak. Packets terminating at the application's own IPv6 addresses remain on its separate transport; this hook also prevents using the exit forwarder as a host route to another tailnet node. A control update that unexpectedly gives the forwarder a narrower IPv6 subnet route gets the same refusal. No changes to `RouteTable::classify` are needed.

**Host-dial invariant:** retain TCP `new_v4` and UDP `0.0.0.0` bind (`F/ts_forwarder/src/dialer.rs:151,157`), Claude's destination boundary, and proxy refusal of IPv6 (`:277`). The new ingress hook never calls a dialer for IPv6. An explicit `if !dst.is_ipv4()` guard in HostExitDialer would make the existing OS-enforced invariant clearer, but is additional defense, not the rejection mechanism.

**Edges:** the helper validates complete TCP/UDP checksums, walks extension headers, never answers RSTs or multicast/unspecified endpoints, and limits ICMP quotes to 1232 bytes (1280-byte total). It resets a first-fragment SYN with a complete TCP header without waiting for reassembly; its full transport checksum is unavailable, so that path relies on the caller's authentication/source/ACL checks. Later fragments and fragmented UDP drop deterministically. Malformed/ACL-denied traffic remains allowed to drop silently; a claim that *arbitrary* fragmented/malformed TCP must always receive RST would conflict with the existing filter's intentional drops. The first ordinary, admitted SYN gets a wire reply immediately, independently of client RTO. Scheduling/tunnel congestion still add latency; no implementation can promise a wall-clock deadline under loss or overload.

### No-tailnet validation

Executed in detached worktree with `CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target`:

* `cargo test -p geiserx_ts_forwarder --locked`: **57 passed** (52 existing + 5 new), including first-SYN peer connect failure within a 250 ms test deadline, correct RST tuple/sequence/checksum, ICMPv6 code/quote/checksum/MTU, extensions/first SYN fragment, and native AnyIP unmatched-RST versus wildcard SYN-ACK. The peer API currently surfaces a reset connect as `Internal(BadSocketHandle)`; the wire RST and prompt completion are separately asserted. This is a preexisting API error-reporting defect, not an iPad protocol dependency.
* `cargo check -p geiserx_ts_runtime --locked`: passed with ingress-hook prototype.
* `cargo test -p geiserx_ts_tunnel --test key_replacement_probe --locked`: passed the **baseline bug reproduction**, described below. Its separate reproduction patch is [codex-02-key-replacement-probe.patch](codex-02-key-replacement-probe.patch); passing means vulnerability reproduced, not fixed.

Before advertising, add configuration tests for `Reject/Off × enable_ipv6 false/true`, both live setters, deduplication, and preservation of explicit IPv4 subnets. Replace the contradictory existing expectations at `F/ts_control/src/config.rs:894,908,1203` and `F/ts_runtime/src/lib.rs:1788`. Add a native unmatched-UDP versus bound-UDP probe and drive the actual ingress hook with a dialer that fails the test if invoked.

Dataplane already has real encrypted pair tests, e.g. `F/ts_dataplane/src/lib.rs:5488`; extend that pattern: legitimate `fd7a.../128` source plus `2000::/3` filter reaches only the rejection transport; forged other-peer source reaches neither rejection nor host dial; unauthorized SYN cannot egress; reply destination routes to the originating peer `/128`; self accepted `::/0` splits to forwarder with either IPv6 flag. Do not use a later fragment/TCP ACK as the unauthorized-SYN test because Go-style pre-ACL exceptions intentionally admit those. These configuration/runtime/dataplane additions are proposed, **not executed**. No official-client or live control-policy test has run.

## 3. Blocker 2: static key replacement / expiry

**Reachable; a successful new-key handshake is not required.** Peer DB preserves runtime `PeerId` by stable node ID while updating the node-key index (`F/ts_runtime/src/peer_tracker/peer_db.rs:129,149`). Full peer changes and patches can change the key; patch path explicitly applies it (`F/ts_runtime/src/peer_tracker/mod.rs:1914`). Dataplane upserts that same ID (`F/ts_runtime/src/dataplane.rs:466,475`). Ordinary WireGuard ephemeral rekeys with an unchanged static key are a different event and should keep legitimate previous receive keys.

Endpoint static-key replacement updates only index/config (`F/ts_tunnel/src/endpoint.rs:1031,1038`). Data dispatch uses receiver session ID, not the current static-key index (`:1189,664`). The established old session therefore still authenticates. The fresh MAC sender is initialized only in `Peer::new` (`:386,393`), and its MAC1 key derives from the *peer static key* (`F/ts_tunnel/src/macs.rs:63,65`). A response after replacement is still MACed for the old peer (`F/ts_tunnel/src/handshake.rs:305`). **The reproduction established A↔B, replaced A's stored B key with C's under the same ID, delivered a fresh B data packet successfully, attempted C's handshake (response rejected), and delivered another B data packet successfully.** Fixing only the MAC sender would still retain old `recv` as `recv_prev` on activation (`F/ts_tunnel/src/endpoint.rs:125,136`).

Runtime reachability is conditional on an ingress path surviving: direct data attribution resolves disco key to the same ID (`F/ts_runtime/src/direct.rs:622`); node-key rotation need not rotate disco keys. Expiry clears advertised endpoints/home DERP but leaves disco keys and routes (`F/ts_control/src/expiry.rs:259`). DirectManager keeps nodes with a disco key (`F/ts_runtime/src/direct.rs:1429`), and netmap reconciliation leaves learned candidates intact (`F/ts_magicsock/src/sock.rs:948`; `F/ts_magicsock/src/path.rs:134`). Thus a learned direct path can survive expiry. Removal/changed DERP node-key mapping can prevent some paths, but is not reliable cryptographic revocation. An established receive key lasts the unexpired part of **180 seconds** (`F/ts_tunnel/src/session.rs:378`; session selection `F/ts_tunnel/src/endpoint.rs:255`). That is a session-age limit, not a hard deadline from revocation: a retained pending old-key handshake can still finish (`F/ts_tunnel/src/endpoint.rs:612`) and install fresh session keys. This cannot create a handshake for an unauthorized brand-new peer; it prolongs an already authenticated revoked identity. A control removal followed by creation with a different stable ID takes the safe deletion path; a same-stable-ID key replacement takes the vulnerable path. The exact hosted-control rotation sequence was not observed in a live tailnet.

Smallest robust fix: **shutdown and replace the whole tunnel peer on static key or PSK change**, preserving the existing return value and unchanged-key keepalive update behavior. Reset sessions, pending handshakes/queues, MAC/cookie state, timestamp replay state and timers together. Conceptual insertion before the current upsert match:

```diff
--- a/ts_tunnel/src/endpoint.rs
+++ b/ts_tunnel/src/endpoint.rs
@@
     pub fn upsert_peer(&mut self, id: PeerId, mut cfg: PeerConfig) -> Option<PeerConfig> {
+        if self.peers.get(&id).is_some_and(|p|
+            p.config.key != cfg.key || p.config.psk.as_ref() != cfg.psk.as_ref())
+        {
+            let mut old = self.peers.remove(&id).expect("peer found above");
+            old.shutdown(&mut self.state);
+            self.state.ids.remove_peer(&old.config.key);
+            assert!(self.state.ids.add_peer(id, &cfg.key), "nodekey collision");
+            self.peers.insert(id, Peer::new(id, cfg));
+            return Some(old.config);
+        }
         match self.peers.get_mut(&id) {
```

`shutdown` already releases session/handshake IDs and cancels timers (`F/ts_tunnel/src/endpoint.rs:913`). The existing API permits a key-collision panic; validate a new key's ownership **before removal** if hardening this path, rather than making that behavior worse. Avoid partial resets.

Also remove expired peers from the tunnel instead of upserting their broken keys (`F/ts_runtime/src/dataplane.rs:475`), and return no installed routes for expired nodes from the shared `Node::routes_to_install` predicate (`F/ts_control/src/node.rs:494`) so source and outbound tables agree. Keep expired nodes in status/WhoIs. Remove expired disco peers from direct reconciliation too for a clean path lifecycle. Recovery on extended expiry must reinsert fresh tunnel state.

Estimate: **40–80 production lines, 3–4 files**, plus **100–200 test lines; roughly half a day** including tests. Files: `ts_tunnel/src/endpoint.rs`, `ts_runtime/src/dataplane.rs`, `ts_control/src/node.rs`, optionally `ts_runtime/src/direct.rs`. Low implementation complexity, medium lifecycle risk: in-flight flows restart across true identity changes. Tests must prove old encrypted data/old handshake responses are refused immediately, new static key handshakes succeed, same-key ephemeral rekeys still work, PSK change resets, expiry/recovery works, and session/timer IDs are reclaimed. Reliable delivery of the revocation remains dependent on blocker 5.

## 4. Blocker 5: lossy security state

`F/ts_runtime/src/env.rs:451` spawns default MessageBus. Actual `kameo_actors-0.4.0/src/lib.rs:41` defaults to `BestEffort`; `src/message_bus.rs:195` uses `try_send` and ignores mailbox-full errors. Default actor mailboxes have capacity **64**, `kameo-0.19.2/src/actor.rs:44,447`. All security hops use it: control raw updates (`F/ts_runtime/src/control_runner.rs:1899`), peer snapshots, source/route/filter publications, dataplane subscriptions (`F/ts_runtime/src/dataplane.rs:281`). Register is only an enqueued tell (`F/ts_runtime/src/env.rs:499`); there is no snapshot replay. A lost withdrawal can leave stale permission indefinitely until a later sufficiently complete update arrives.

**Do not replace raw `StateUpdate` with a watch channel.** These messages carry deltas and optional filter updates (`F/ts_control/src/tokio/map_stream.rs:120,129`; accumulation `F/ts_runtime/src/packetfilter.rs:286`); coalescing loses operations. Nor is `PeerState` fully self-contained for its current consumers: dataplane applies its delta `upserts/deletions`, not a full reconciliation (`F/ts_runtime/src/dataplane.rs:466,490`). Watching the latest `PeerState` without changing that consumer still loses revocations. Existing `LiveFilterRx` only protects the updater→DoH consumer; the raw input and dataplane publication remain lossy (`F/ts_runtime/src/packetfilter.rs:313,315`).

**Smallest fix specifically for delivery loss:** ordered `Guaranteed` bus delivery **with an unbounded broker mailbox**, registration acknowledgments, and explicit subscriber-ready barriers before cached/live control publication. A one-line `BestEffort -> Guaranteed` swap on the current bounded broker is risky: the broker can block on a subscriber whose current handler is itself awaiting publication into a full broker mailbox. An unbounded broker makes those publish enqueues nonblocking; it does not make subscriber handling unbounded. Example construction:

```diff
--- a/ts_runtime/src/env.rs
+++ b/ts_runtime/src/env.rs
@@
-            bus: MessageBus::spawn_default(),
+            bus: MessageBus::spawn_with_mailbox(
+                MessageBus::new(kameo_actors::DeliveryStrategy::Guaranteed),
+                kameo::mailbox::unbounded(),
+            ),
@@
-            .tell(Register(slf.clone().recipient::<M>()))
+            .ask(Register(slf.clone().recipient::<M>()))
```

Then retain actor refs and await a readiness message after `on_start` has registered them, **before `ControlRunner::spawn`**, including its cached-map replay (`F/ts_runtime/src/lib.rs:254,275,281,381`; `F/ts_runtime/src/control_runner.rs:197,1604`). Critical actor failure must shut down/refuse the dataplane rather than quietly remove the subscriber and continue on stale permissions. `Guaranteed` only acknowledges mailbox delivery, not application; initial readiness and revocation application tests must distinguish them.

Estimate: **80–150 production lines, 5–8 runtime files, roughly 1 day** with mailbox saturation/startup/deadlock/failure tests. Smallest in code, medium risk: slow subscribers cause broker backlog and memory growth; taking all telemetry through it adds coupling. Larger mailbox capacity, spawned deliveries, retries that reorder deltas, or warning-and-dropping are not correctness fixes.

**Preferred bounded design:** accumulate every control delta in a single lossless ordered state owner; publish a **complete generation-tagged security snapshot** (peers/expiry, source ownership, ACL, inbound/outbound routes) and atomically apply it under the dataplane lock before processing further packets. A watch is safe only *after* accumulation, with full peer reconciliation and application acknowledgment. Keep unrelated telemetry on the best-effort bus. This additionally fixes transient mixtures of old ACL/source tables and new identities; the small delivery fix alone does not make separately derived tables atomic. Estimate **250–450 production lines across 6–9 files, 2–3 days plus 150–300 test lines**. Tests: revoke while mailboxes are saturated; two consecutive disjoint peer deletions/filter deltas; subscriber starts late; paused consumer catches up to the full latest state; no packets process a mixed generation; subscriber death fails closed; no publication deadlock.

## Handoff order

Land IPv6 ingress rejection **before** paired advertisement; include startup/live advertisement tests; then reject expired/reset changed identities and make security updates reliable before declaring a safe IPv4 exit baseline. I agree with the revised paired-default target and disagree with PLAN Phase 1.7's blanket “do not advertise `::/0` until working”: explicit rejection is the correct working IPv6 behavior for this IPv4-only host. The isolated prototype establishes the mechanism, not the final iPad/control-plane acceptance gate.
