# NAT port mapper (NAT-PMP + PCP) for the Rust node: plan v2

Status: revised plan, not implemented. Supersedes `docs/PORTMAPPER-PLAN.md` (v1) for phases 0
and 1. It incorporates every finding of `docs/review-36-portmapper-plan.md` (review 36) and
Kasper's decisions on the five departures. Bead: `tsr-x4yg`.

- Go reference: `repos/tailscale-go` (`net/portmapper`, `wgengine/magicsock`, `net/netcheck`,
  `net/netmon`).
- Rust tree: `repos/geiserx-tailscale-rs`. Implementation branch `feat/portmapper` in the
  worktree `scratch/portmapper`, created from `origin/main`.
- v1 and review 36 cite line numbers on `ts-exit/main` (`5d1f88f`). `origin/main` (`69713c9`)
  lacks 19 ts-exit commits (smoltcp recovery, control watchdog, exit_node port forwarding), so
  line numbers drift. This plan names functions; re-resolve lines on the branch.
- Scope: **phases 0 and 1 only (NAT-PMP and PCP).** UPnP-IGD is phase 2 and out of scope. No
  UPnP code is written now, but §3.10 keeps the design ready for it.

## 0. Summary

### 0.1 Why
kmini sits behind an Asus router running MiniUPnPd, which offers NAT-PMP, PCP and UPnP. A client
behind a hard NAT can't reach kmini's STUN endpoint, so the path stays on DERP. Go asks the router
for an inbound mapping and advertises it as endpoint type 3 (`EndpointPortmapped`).

This was verified on 2026-10-17. A Go `tailscaled` on the same Mac holds a NAT-PMP mapping (UDP
41644), and a hard-NAT laptop reaches it **directly**. kmini, which has no mapper, stays on the
Frankfurt relay. Go picks NAT-PMP on this router.

The goal of phases 0–1: the hard-NAT laptop gets a direct path to kmini.

### 0.2 Decisions on the departures (review 36 recommendations adopted)
| # | Departure | Decision | Where |
|---|---|---|---|
| a | Suppress private/CGNAT external addresses | **Config option.** The library default matches Go: advertise everything. The exit-node runner turns suppression on. A suppressed mapping is kept and renewed for cleanup, not recreated. | §3.8, §3.11 |
| b | Renew while idle, while peers exist | **Config option.** The library default is Go-like (lazy). The runner turns it on. With no peers, a mapping is left to expire. | §3.6, §3.9 |
| c | Bind LAN sockets to the host's own IP **and interface** | **Yes**, release sockets included. | §3.3 |
| d | Destination allow-list via a validated gateway capability | **Yes.** Every LAN send goes through it. | §3.3 |
| e | Epoch check on every reply | **Deferred** to a later phase (v1's phase 3). Phases 0–1 keep Go's check, which runs on probe replies only. | §3.7 |

### 0.3 What review 36 changed (index)
| Review 36 finding | Resolution in v2 |
|---|---|
| H1: stale create/renew resurrects a mapping | Generations, cancellation, stale-commit rejection, an owned-rule ledger, an ordered close, a sync cleanup handle (§3.5, §3.12) |
| H2: renewal storm; no activity-independent expiry | `next_attempt_at` / `renew_after` / `expires_at` kept separately; spacing enforced across all triggers; an expiry timer; loss is published (§3.6, §3.9) |
| H3: "LAN only" is not a boundary | `GatewayCap` with on-link validation, interface binding, a destination allow-list, a policy check on each send; test transport kept separate (§3.3) |
| H4: overstated router-state bounds | Honest bounds stated (§4); short probe leases; permanent-UPnP policy fixed now for phase 2 (§3.10) |
| M5: gateway lookup contract | Typed `Result<Option<Gateway>, LookupError>`; default-route and metric selection; bounds-checked macOS parser; fixtures (§3.2) |
| M6: unbounded staleness without netmon | Synchronous invalidation on rebind, network down and failed rebind; a bounded 10 s route poll independent of the idle gate; network identity includes the interface (§3.7) |
| M7: endpoint ordering and STUN evidence | Explicit priority; de-duplication by address with PortMapped winning; STUN facts carried separately (§3.8) |
| M8: disable semantics and packet validation | Policy checked at every send and commit; in-flight work cancelled; `only-tcp-443` stops all LAN I/O; full PMP/PCP validation; PCP nonces from a CSPRNG; local port 0 rejected (§3.4, §3.6) |
| Go-analysis corrections | §1 |
| CI text scan is not enforcement | Type boundary plus runtime denial tests plus dependency-graph check; the scan is kept only as a tripwire (§5) |

---

## 1. Go analysis: v1 §1–§2 stand, with these corrections

v1 §1 (how Go's portmapper works) and §2 (how Go integrates it) remain the reference. Review 36
verified them. The following corrections override v1:

1. **Protocol selection.** PCP is **not** a sequential fallback after PMP fails.
   `preferPCP = !DisablePCP && (DisablePMP || (!recentPMP && recentPCP))`. An explicit PMP or PCP
   error returns at once. Only a read failure or timeout falls through, and it falls through to
   UPnP. Rust phase 1, which has no UPnP, returns "no mapping" there.
2. **Probe source check.** Go's probe accepts PxP replies by **source port only**; creation checks
   the whole gateway tuple. Rust checks the exact `gw:5351` everywhere (§3.4).
3. **Early exit.** The early-exit condition accepts any PCP ANNOUNCE reply, not only success.
4. **Releases aren't all fire-and-forget.** UPnP release is synchronous SOAP under the client
   mutex. PxP releases are unacknowledged sends. Rust bounds every release (§3.12).
5. **Events.** In this Go snapshot, PMP success and the UPnP success branches return a nil
   mapping object, so `createMapping` returns before publishing the event. Only PCP publishes.
   Rust notifies on success, loss, expiry and disable for **all** protocols. This is a bug fix, not
   parity. `TestUpdateEvent` covers PCP only and checks only that an event arrives.
6. **Env knobs.** Go `envknob` values are cached; they aren't fresh OS-env reads on every attempt.
   `TS_DISABLE_UPNP` blocks mapping, but Probe's SSDP branch checks only
   `DebugKnobs.DisableUPnP` (phase 2 detail).
7. **Epoch bound.** "Up to 10 min" for reboot detection holds only under active probing. It is
   not an upper bound when idle.
8. **SSDP (phase 2).** The reply cap is effectively 11 (`len > 10`); Rust uses a strict 10. The IGD
   marker is required even from port 1900.
9. **Linux gateway heuristic.** Go takes the first `UP|GATEWAY` route with a private gateway. It
   doesn't check destination, mask or metric, so it can pick a VPN or static-route gateway. Rust
   doesn't copy this (§3.2).
10. **PMP delete.** Go sends the current external port with lifetime 0. RFC 6886 §3.4 says the
    suggested external port MUST be 0. Rust follows the RFC (harmless deviation, §3.4).

---

## 2. Goals, non-goals, invariants

**Goals (phases 0–1).**
- Probe NAT-PMP and PCP.
- Create, renew and release one UDP mapping for the underlay port.
- Advertise the mapping as `PortMapped` (type 3).
- Report the NetInfo facets `PMP`, `PCP` and `HavePortMap`.
- Gate CallMeMaybe on it.
- Release it on graceful shutdown.
- Provide a one-shot `--portmap-probe` for validating the deployment context.

**Non-goals now:**
- UPnP-IGD and SSDP (phase 2).
- Epoch checks on every reply.
- Listening for PMP multicast announcements.
- Persisting ownership across restarts.
- An OS link monitor.
- IPv6 mappings.

**Invariants (testable, kept from v1 and tightened):**
1. **IPv4 only.** No `[::]` socket. An IPv6 gateway is refused. A PCP external address that isn't
   IPv4-mapped is rejected.
2. **LAN I/O only through `GatewayCap`.**
   - Destination is `gw:5351` only in phases 0–1.
   - The gateway must be private and on-link.
   - Sockets are bound to the host's own IP and its interface.
   - The policy is checked at every send.
3. **No mapping for local port 0**, ever. A PCP or PMP request with internal port 0 means "all
   ports".
4. **An external port of 0 never becomes an endpoint or a delete target.**
5. **Zero I/O when disabled.** This covers config `disable_port_mapper`, `TS_DISABLE_PORTMAPPER`
   and `only-tcp-443`. For the first two no client is even constructed.
6. **Tests never touch the real network.** Shared test configs disable the mapper, and the default
   test lookup panics if it is ever called.
7. **The mapper is not a forwarding path.** Its sockets are control sockets, separate from
   forwarded exit flows. They never become a `RealDialer` bypass (`AGENTS.md`, proxy-egress
   section). No changes to `ts_forwarder` or `ts_host_net`.

---

## 3. Design

### 3.1 Crate layout: `ts_portmapper` (package `geiserx_ts_portmapper`)
```
ts_portmapper/
  src/lib.rs            pub API: Client, Config, Policy, PortmapState, ProbeReport, Error
  src/gateway/mod.rs    Gateway, LookupError, GatewayLookup trait, selection + validation
  src/gateway/darwin.rs sysctl NET_RT_DUMP2 fetch + bounds-checked parser (pure parser split out)
  src/gateway/linux.rs  /proc/net/route parser (pure) + reader
  src/gateway/other.rs  Err(LookupError::Unsupported) on every other target
  src/cap.rs            GatewayCap: the only module that creates sockets or sends
  src/pmp.rs            NAT-PMP codec (pure)
  src/pcp.rs            PCP codec (pure)
  src/oneshot.rs        minimal one-shot client (phase 0); reused by the lifecycle client
  src/client.rs         lifecycle state machine (phase 1)
  src/schedule.rs       next_attempt_at / renew_after / expires_at / backoff (pure, phase 1)
  src/owned.rs          owned-rule ledger (phase 1)
  src/testing/          fake router harness + TestCap (cfg(any(test, feature = "test-harness")))
  fuzz/                 cargo-fuzz targets, same layout as ts_tka/fuzz
```
- No `upnp` module or feature now. §3.10 lists the extension points.
- Non-macOS/Linux targets compile and get `Unsupported`, so a default-on feature never breaks the
  build.
- `ts_control` must **not** depend on `ts_portmapper`. Control receives a transport-neutral
  `PortMapFacets` defined in `ts_control` (§3.8).

### 3.2 Gateway lookup contract (finding M5)

```rust
pub struct Gateway {
    pub gw: Ipv4Addr,          // private (RFC 1918), on-link
    pub self_ip: Ipv4Addr,     // private, configured on `if_index`, prefix contains `gw`
    pub prefix_len: u8,
    pub if_index: u32,
    pub if_name: String,
}
pub enum LookupError {
    Unsupported,
    Permission(io::Error),            // EPERM/EACCES from sysctl or open(); kept verbatim
    Os(io::Error),                    // other syscall/file errors, verbatim
    Parse { offset: usize, what: &'static str },
    TooLarge,                         // dump/file exceeded the cap after retries
    GatewayIpv6,                      // best default route is IPv6-only
    NotPrivate,                       // best default route's gateway is public/CGNAT/etc.
    NotOnLink,                        // no address on the route's interface contains the gateway
    InterfaceUnusable,                // down, loopback, point-to-point, or vanished
    Ambiguous,                        // tie between equally preferred default routes/addresses
}
pub trait GatewayLookup: Send + Sync { fn lookup(&self) -> Result<Option<Gateway>, LookupError>; }
```
`Ok(None)` means "no IPv4 default route": offline, which is normal. Every error is diagnosable. The
probe prints errors verbatim, and the runtime logs each distinct error once per network generation.

**Selection rules (both platforms):**
1. Collect IPv4 routes with destination `0.0.0.0`, mask `0`, `UP` and `GATEWAY` flags.
   - macOS: skip `RTF_IFSCOPE` routes. On Linux this means main-table entries.
2. Pick the single most preferred default route. That is the **lowest metric** on Linux, and the
   unscoped default on macOS.
   - An exact tie → `Ambiguous`.
   - If the best route's gateway is not private → `NotPrivate`. **Never fall through** to a
     less-preferred private route; that's how Go can pick a VPN gateway.
   - If only an IPv6 default exists → `GatewayIpv6`.
3. Resolve the interface by index and name.
   - It must be `UP`, not loopback, and not point-to-point. A utun or WireGuard default route
     fails here.
4. The self address must be an IPv4 address **on that interface** whose prefix contains the
   gateway.
   - macOS: prefer the route's `RTAX_IFA` sockaddr when present and validate it against the
     interface.
   - Several candidates → cross-check with the kernel's source selection: a `UdpSocket` bound
     `0.0.0.0:0` and `connect()`ed to `gw:5351` (no packet is sent), then `local_addr()`. If that
     disagrees with every candidate → `Ambiguous`.
   - None → `NotOnLink`.
5. The self IP must be private. Link-local, CGNAT, loopback and unspecified are rejected.

**macOS parser** (`sysctl(CTL_NET, PF_ROUTE, 0, AF_INET, NET_RT_DUMP2, 0)`):
- Fetching:
  - Size query, then allocate size × 1.25.
  - Retry on `ENOMEM` up to 3 times.
  - Hard cap 4 MiB → `TooLarge`.
  - The only `unsafe` is the sysctl call itself.
- Parsing is pure, over `&[u8]`:
  - Iterate messages by `rtm_msglen` (u16 LE at offset 0). It must be ≥ the header length and
    ≤ the remaining bytes, and `rtm_version == RTM_VERSION` (5); otherwise `Parse`.
  - Read the header fields with `from_ne_bytes` at fixed offsets. A test asserts those offsets
    against `core::mem::offset_of!(libc::rt_msghdr2, …)` and `size_of`.
  - Walk sockaddrs per the `rtm_addrs` bitmask. Each takes
    `ROUNDUP(sa_len) = sa_len == 0 ? 4 : 1 + ((sa_len - 1) | 3)` bytes.
  - Never read past `sa_len` or the message end.
  - Compact netmasks (`sa_len < 16`, family 0) are zero-extended.
  - Unknown families are skipped by length.
  - **No casting of byte buffers to structs.**
- Keep Go's `TestFetchRoutingTable` repeated-fetch regression: 10 consecutive fetches succeed and
  agree.

**Linux parser** (`/proc/net/route`):
- Read at most 64 KiB / 1000 lines; beyond that → `TooLarge` (Go's `TestExtremelyLongProcNetRoute`).
- Parse the header to find column positions. Destination, gateway and mask are little-endian hex.
- Interface via `if_nametoindex`; addresses via `if-addrs` (already locked).
- Documented limitation: `/proc/net/route` shows only the main table. Policy routing
  (`ip rule` / fwmark) can route the gateway elsewhere; the source-selection cross-check in step 4
  catches the common case. A netlink backend is a later improvement.

**Override.** `port_map_gateway = (gw, Option<self_ip>)`:
- It replaces only *discovery*. It goes through the same validation (private, on-link,
  interface) as discovered routes.
- It is **not** the test transport. Tests use `TestCap` (§3.3), which production code can't
  construct.

**Fixtures:**
- Synthesized byte dumps plus sanitized captures. Any non-private address in a capture is
  rewritten into 203.0.113.0/24 or 198.51.100.0/24 **before** it is committed.
- Cases:
  - several defaults (scoped and unscoped);
  - a utun default with no `RTF_GATEWAY`;
  - a VPN (point-to-point) default carrying RFC 1918;
  - overlapping subnets on two interfaces;
  - a public-gateway default with lower metric than a private one → `NotPrivate`, not the private
    one;
  - an IPv6-only default;
  - an interface that disappears between dump and resolve;
  - truncated and garbled buffers (`Parse`, never a panic);
  - the Linux hex byte order;
  - the Linux line cap.

### 3.3 `GatewayCap`: the validated gateway capability (finding H3, departures c and d)
`GatewayCap` is created once per **network generation** from a validated `Gateway`. It is the only
code in the crate that creates sockets or sends.

```rust
pub struct GatewayCap { /* private: Gateway, generation, policy handle, bind mode, valid flag */ }
impl GatewayCap {
    pub(crate) fn validate(gw: Gateway, gen: NetGen, policy: PolicyHandle) -> Result<Arc<Self>, CapError>;
    pub(crate) async fn pxp_exchange(&self, req: &[u8], deadline: Instant, accept: impl FnMut(&[u8]) -> Verdict) -> Result<Reply, CapError>;
    pub(crate) fn pxp_send_blocking(&self, req: &[u8]) -> Result<(), CapError>; // Drop-path release
    pub(crate) fn revalidate(&self, lookup: &dyn GatewayLookup) -> bool;        // same gw/self/if still current
    pub(crate) fn invalidate(&self);                                             // sends refuse afterwards
}
```
- **Destination allow-list.** Phases 0–1 allow exactly one destination, `gw:5351`, through a
  `LanDest` enum with a single `Pxp` variant. There is no API that accepts an arbitrary
  `SocketAddr`. Phase 2 adds variants (§3.10).
- **Binding (departure c):**
  - Each exchange uses a fresh UDP socket bound to `self_ip:0`. The source is then
    PCP-client-IP-consistent and fails closed if the address vanishes.
  - It is also bound to the interface:
    - macOS: `IP_BOUND_IF` (`socket2::Socket::bind_device_by_index_v4`, unprivileged).
    - Linux: `SO_BINDTODEVICE`. On `EPERM` it falls back to source-IP binding plus revalidation
      before each send.
  - The bind mode is recorded and printed by the probe.
  - The same applies to release sockets.
- **Policy at each send.**
  - Before every `send_to`, `pxp_exchange` checks:
    - `policy.lan_sends_allowed()` (disable-all and `only-tcp-443`);
    - the cap's own `valid` flag (cleared on invalidation);
    - a closing check.
  - A denied send returns `CapError::Denied` without touching the socket.
- **Receive.**
  - Accept only datagrams whose source is exactly `gw:5351`.
  - Receive into a buffer of `cap + 1` bytes (1101 bytes, the PCP maximum of 1100 plus one). A
    datagram that fills it is treated as oversize and dropped.
  - Junk doesn't extend the **absolute** deadline (250 ms per exchange, Go parity).
  - At most 64 datagrams are examined per exchange.
- **Errors are kept verbatim** as `CapError::Io { kind, raw_os_error }`, for example
  `EHOSTUNREACH` and `EPERM`. This matters for diagnosing macOS Local Network privacy. They are
  not relabelled as "no services".
- **Test transport.** `TestCap` lives in `testing/`, behind `cfg(any(test, feature =
  "test-harness"))`.
  - It maps logical gateway addresses to loopback listener ports. Production code can't reach it,
    and the `test-harness` feature is never enabled by `ts_runtime`'s normal features.
  - Both `GatewayCap` and `TestCap` implement a crate-private `PxpTransport` trait, so the
    state-machine code is identical in both.
- **Residual attack surface, documented.** NAT-PMP is unauthenticated. An on-LAN spoofer can:
  - fake service availability;
  - force failures and churn;
  - report a false external IP, which becomes a ping target for peers.

  PCP nonces stop off-path spoofing, not an observer. Mitigations:
  - bounded parse work and logs;
  - absolute deadlines;
  - the mapping target is always our own `self_ip` and our local port. A reply can never choose
    the internal target.

### 3.4 Codecs and validation (finding M8)
Both codecs are pure, own their byte layouts, and never panic (fuzzed).

**NAT-PMP (RFC 6886).**
- Build:
  - public-address request `{0, 0}`;
  - MAP-UDP `{0, 1, 0, 0, internal, suggested_ext, lifetime}`;
  - delete: MAP-UDP with lifetime 0 and suggested external port 0 (RFC; see §1 item 10).
- The builder refuses `internal == 0`. It is a typed `NonZeroU16`.
- Parsing a reply requires all of:
  - version 0;
  - opcode = request opcode + 128;
  - source `gw:5351` (checked by the cap);
  - for success: exact length (12 for the address reply, 16 for MAP).
- Error replies: result ≠ 0 with ≥ 8 bytes is a typed `PmpError(code)` (codes 1–5 and unknown).
  It is never a success.
- Address reply: `0.0.0.0` → invalid.
- MAP success also requires:
  - **internal port == the requested internal port**;
  - external port ≠ 0 when the requested lifetime > 0;
  - lifetime > 0 for a create or renew.
  - A delete reply has lifetime 0.
- The epoch (seconds since start of epoch) is returned for the probe-only epoch check.

**PCP (RFC 6887).**
- Build:
  - ANNOUNCE: a 24-byte header carrying `::ffff:self_ip`;
  - MAP: protocol 17, `internal: NonZeroU16`, suggested external port (previous one, or 0),
    suggested external address `::ffff:0.0.0.0`, 12-byte nonce;
  - delete: lifetime 0, port 0, all-zero mapped address, **original nonce** (Errata 3621).
- Nonces come from the OS CSPRNG (`getrandom`), never a seeded PRNG. A renewal reuses the
  mapping's nonce.
- Parsing a reply requires all of:
  - version 2;
  - **R bit set**;
  - opcode matches the request;
  - length ≥ 24, a multiple of 4, ≤ 1100;
  - source `gw:5351`.
- A short error response (header only) is accepted as a typed error carrying its **lifetime**,
  which is the error-cache duration (§3.6).
- MAP success requires all of:
  - length ≥ 60;
  - nonce == ours;
  - protocol == 17;
  - internal port == requested;
  - assigned external address IPv4-mapped and not `0.0.0.0`;
  - external port ≠ 0;
  - lifetime > 0.
  - Options are skipped with bounds checks; we send none.
  - A MAP reply to a delete has lifetime 0.
- ANNOUNCE:
  - Result OK → `pcp=true`.
  - `NOT_AUTHORIZED` (2) or `ADDRESS_MISMATCH` (12) → `pcp=false`. These are logged at warn
    once per generation.
  - Others → `pcp=false` and logged.
- Result codes are a typed enum covering all RFC 6887 values.

**Lifetimes.** Lifetimes are `u32`. Instants use `checked_add`, clamped to `Instant::now() + 1
year`. `u32::MAX` must not panic.

### 3.5 The lifecycle client: generation-guarded state (finding H1)
One `std::sync::Mutex<State>`, never held across `.await`.

```rust
struct State {
    gen: u64,                          // bumped by every invalidation/disable/close/port change
    phase: Phase,                      // Running | Closing | Closed
    local_port: Option<NonZeroU16>,    // None → no attempts (port 0 rejected at set_local_port)
    net: Option<NetSnapshot>,          // { net_gen, cap: Arc<GatewayCap>, gateway: Gateway }
    probe: ProbeCache,                 // pmp/pcp seen_at + epoch + pmp public ip, last_probe_at
    mapping: Option<Mapping>,          // { proto, external, internal, nonce?, net_gen, schedule }
    sched: Schedule,                   // §3.6
    owned: OwnedRules,                 // §3.5.2, bounded
    inflight: Option<Inflight>,        // { gen, kind, abort: AbortHandle }
}
```

**3.5.1 Attempt protocol.**
1. **Begin, under the lock.**
   - Refuse unless all of these hold:
     - `phase == Running`;
     - the policy allows mapping;
     - `local_port.is_some()`;
     - `net.is_some()`;
     - `now ≥ sched.next_attempt_at`;
     - `inflight.is_none()`;
     - `owned` has room.
   - Snapshot `AttemptCtx { gen, cap, local_port, proto, prev_ext_port, nonce }`.
   - Record a **pending owned rule** *before* sending (§3.5.2).
   - Set `inflight`. Release the lock.
2. **I/O**, through `ctx.cap`. The cap re-checks the policy and its validity at each send.
3. **Commit, under the lock.**
   - If `ctx.gen != state.gen` or `phase != Running` → **reject**: do not install anything, mark
     the owned rule `Orphaned` (cleaned up as in §3.5.2), and return `Error::Stale`.
   - The same check guards the probe-cache update: a stale probe result is dropped.
   - Otherwise install the mapping and update the schedule.
   - Publish a new `PortmapState` (§3.8).
4. **Inflight clearing** is done by an RAII guard's `Drop` (`InflightGuard`), so success, error,
   panic and abort all reset single-flight.
   - Background attempts run in the client's `JoinSet`.
   - Finished entries are reaped with `try_join_next` at every attempt start and driver tick.

**Invalidation** happens on:
- `set_local_port(new != old)`;
- a network-generation change;
- `note_network_down`;
- an epoch rollback;
- disable;
- `close`.

In each case, under the lock: bump `gen`, abort `inflight` (`AbortHandle::abort`), take
`mapping` out, call `cap.invalidate()` if the network changed, and publish loss. Releases
(§3.12) happen after unlocking.

**3.5.2 Owned-rule ledger** (`OwnedRules`, max 4 entries). This bounds outstanding router state,
not just tasks.
- An entry is added **before** each MAP send:
  - fields `{ proto, net_gen, gw, self_ip, internal, requested_ext, nonce (PCP), state: Pending, sent_at, router_expiry: None }`;
  - on a committed reply → `state: Active`, `router_expiry = now + granted`;
  - on a stale commit, or an invalidation while pending or active → `Orphaned`.
- A **renewal** of the same rule updates the entry in place: same internal port, and the same
  nonce for PCP.
- A protocol switch (PMP rule active, PCP chosen) or a changed internal port leaves the old entry
  `Orphaned`.
- Cleanup of `Orphaned` entries:
  - The delete is built from the entry: PMP needs only the internal port; PCP needs the nonce and
    the internal port. So a **commit-then-lost-reply** rule is still deletable.
  - Deletes are sent only when **all** of these hold:
    - the policy allows LAN sends;
    - the entry's network still revalidates (same gateway, self IP and interface);
    - the entry is not past `router_expiry`.
  - Otherwise the entry is dropped and logged at info: "left to router expiry, ≤ granted
    lifetime".
- When the ledger is full, new attempts are refused until cleanup or expiry frees a slot (warn
  once).
- A PMP delete is scoped to our host IP and internal port. It can't touch another client's
  mappings or the co-resident Go `tailscaled`'s (internal 41644). This is tested (case M14).

**3.5.3 Close** (`close(budget)`):
1. Set `phase = Closing`, so no new work starts. Bump `gen`.
2. Abort and await inflight work, bounded.
3. Mark the current mapping `Orphaned`.
4. Send the deletes allowed by §3.5.2 within `budget`.
5. Set `phase = Closed`.

Close is idempotent; later calls return immediately.

**Public API (phase 1):**
```rust
impl Client {
    pub fn new(cfg: Config) -> (Self, watch::Receiver<PortmapState>);
    pub async fn probe(&self) -> Result<ProbeReport, Error>;           // cache-respecting
    pub fn get_cached_mapping_or_start_creating_one(&self) -> Option<SocketAddrV4>;
    pub fn have_mapping(&self) -> bool;
    pub fn set_local_port(&self, port: u16) -> Result<(), Error>;     // 0 → Err(LocalPortZero) + invalidate
    pub fn note_network_down(&self);
    pub fn check_network(&self) -> NetCheck;                           // §3.7, run by the driver poll
    pub fn disable(&self, why: DisableReason);                          // §3.6.3
    pub fn schedule(&self) -> ScheduleSnapshot;                         // next due instant for the driver
    pub async fn close(&self, budget: Duration);
    pub fn cleanup_handle(&self) -> CleanupHandle;                      // §3.12, sync
}
pub enum Error { Disabled, NoServices, LocalPortZero, Gateway(LookupError), Cap(CapError),
                 Pmp(PmpError), Pcp(PcpError), Stale, Busy, Backoff, LedgerFull }
```

### 3.6 Scheduling, retry spacing and expiry (finding H2)
`Schedule` is pure and unit-tested with a manual clock:
- `next_attempt_at`: the earliest time any create or renew may start, **whatever the trigger**
  (driver timer, kick, rebind, ReSTUN, `get_cached…`).
- `renew_after`: when a healthy mapping should be renewed.
- `expires_at`: local trust end. After it the mapping is dropped and the endpoint withdrawn.
- `router_expiry`: what the router granted, kept for honesty and diagnostics.

**Leases:**
- Requested lifetime 7200 s (Go).
- `trusted = min(granted, 7200 s)`; `expires_at = commit_time + trusted`.
- `renew_after = commit_time + clamp(trusted / 2, 10 s, 3600 s)`.
- PCP may grant more than requested. We still renew by our trusted schedule, and
  `router_expiry` records the real bound (§4).
- Lifetime 0 on a create or renew is a failure.

**Failures** advance `next_attempt_at = now + backoff(n)`:
- Backoff steps: 10 s, 30 s, 60 s, 2 min, 5 min, then a 10 min cap, each with ±20 % jitter.
- A timeout, no responder, or `NoServices` counts as a failure.
- PMP result 2 (not authorized) and PCP `NOT_AUTHORIZED`: `next_attempt_at ≥ now + 30 min`.
- PCP errors carrying a lifetime: `next_attempt_at ≥ now + min(error_lifetime, 30 min)`
  (RFC 6887 §7.4 / §11.4). Short-lifetime errors still respect the backoff floor.
- A **hard floor of 2 s** between any two attempt starts, even across a network change.
- Success or a network-generation change resets `n`.
- A **failed renewal keeps the mapping** until `expires_at`. Retries follow the same backoff, so a
  past `renew_after` never produces a hot loop. The driver schedules on
  `max(renew_after, next_attempt_at)`, never on a bare past instant.

**Expiry is independent of activity and peers.** The driver always arms a timer for
`expires_at`. When it fires:
- the mapping is removed;
- loss is published, so the advertiser republishes immediately even while idle.

As a second guard, `MagicSock::self_endpoints` drops a stored portmapped endpoint whose
`expires_at` has passed (§3.8).

**Renewal triggers:**
- With `renew_while_idle = false` (library default, Go-like), renewal is attempted only on
  activity-driven ticks:
  - the periodic ReSTUN cadence gated by `should_do_periodic_restun`;
  - kicks.
- With `renew_while_idle = true` (runner), the driver also fires at `renew_after` whenever at
  least one peer exists, ignoring the idle gate.
- With no peers the mapping is never renewed: it expires and is withdrawn.

**3.6.3 Disable semantics (finding M8).**

| Source | Read when | Effect |
|---|---|---|
| Config `disable_port_mapper` (Go `Options.DisablePortMapper`) | Construction | No client, no driver, no socket. NetInfo facets unset (`None`), like Go without a portmapper. |
| `TS_DISABLE_PORTMAPPER` (Go name) | Once at runtime construction (cached, like Go's `envknob`) | Same as config. |
| Control `only-tcp-443` (`Node::only_tcp_443`) → disable-all | Live, on each self-node update | See below. |
| Debug: disable PMP or PCP | Probe CLI only (`--portmap-proto`) | Restricts the protocol set. There is no runtime env for this; Go has none either. |
| Control `debug-disable-upnp` / `TS_DISABLE_UPNP` | Phase 2 | Reserved; not wired now. |

When `only-tcp-443` turns on, `Client::disable(OnlyTcp443)` runs immediately:
- it clears the policy bit that `GatewayCap` checks at **every send** and at commit;
- it bumps `gen` and aborts in-flight work;
- it withdraws the mapping and publishes loss;
- it sends **no further LAN packets, including deletes.** Owned rules are left to router expiry
  (≤ granted lifetime) and logged once at info.

This is the explicit trade-off: `only-tcp-443` means "no UDP", and the delete is UDP. When the
knob turns off again and the same network revalidates, leftover `Orphaned` entries are deleted
first, then normal operation resumes. NetInfo `PMP`/`PCP` become `Some(false)` while disabled
(Go: the probe returns `ErrPortMappingDisabled` after the facets were preset to false).

### 3.7 Network identity, invalidation and epoch (finding M6)
**`NetIdentity = (gw, self_ip, if_index, if_name, prefix_len)`.** Two networks with an identical
identity are indistinguishable. This is documented; the only cost is a delete aimed at an
equivalent-looking router, which deletes only *our* internal port's rule.

Invalidation runs **synchronously** (from the caller's task, before it returns) on:
- every `DirectManager` rebind path: `rebind`, `rebind_and_reprobe`, and a failed socket
  replacement that leaves no bound socket (`MagicSock::rebind` error path);
- `note_network_down`.

The magicsock portmapped endpoint is cleared at the same point. After a successful rebind:
- If the network identity and local port are unchanged, the client may re-install its still-valid
  mapping **without I/O**, and the driver re-publishes it.
- Otherwise the old mapping is invalidated. It is released only if its `GatewayCap` revalidates
  (§3.5.2).

**Bounded route poll until netmon exists.**
- The driver calls `client.check_network()` every 10 s, independent of the STUN idle gate. This
  costs one sysctl or `/proc` read plus `if-addrs`, a few microseconds.
- On a changed identity: bump `net_gen`, invalidate, and release only to a revalidated old network.
- On `Ok(None)` or an error: behave like network-down (no release).
- When OS `LinkMonitor` backends replace `NoopLinkMonitor`, `NetmonSupervisor` calls the same
  `check_network` / `note_network_down` and the poll interval can grow.

**Suspend and clock jumps.**
- `Instant` doesn't advance during sleep on macOS (`CLOCK_UPTIME_RAW`) or Linux
  (`CLOCK_MONOTONIC`).
- Each poll compares the `SystemTime` delta with the `Instant` delta. A discrepancy over 30 s is
  treated as resume or a clock jump. The driver then:
  - runs `check_network`;
  - sets `renew_after = now`, subject to `next_attempt_at`;
  - forces a fresh probe.
- A false positive (NTP step) costs one renewal.

**Epoch (departure e deferred).** Go's rule applies to **probe replies only**:
- Epoch 0 is ignored. An epoch equal to or above the mapping's epoch is fine.
- An epoch that went backwards drops that protocol's mapping without release and clears its probe
  cache.
- The commit-generation check applies, so a stale probe can't drop a newer mapping.

Every-reply checks are a later phase.

### 3.8 Endpoints, NetInfo and control (finding M7, departure a)

**State snapshot.** The client publishes a `watch::Receiver<PortmapState>`:
```rust
pub struct PortmapState {
    pub gen: u64,
    pub mapping: Option<PortmapEndpoint>,   // { external: SocketAddrV4, proto, expires_at }
    pub advertise: bool,                    // after the (a) policy filter
    pub report: ProbeReport,                // { pmp: Option<bool>, pcp: Option<bool>, upnp: None }
}
```
`watch` carries the latest state and may coalesce intermediate ones. Consumers re-read; nothing
promises exactly-once. Every success, loss, expiry, disable and probe change publishes.

**Advertising policy (a).** `advertise_nonpublic_external: bool`. The library default is `true`
(Go). The runner sets it to `false`.
- With `false`, an external address that fails `is_pingable_candidate` or is CGNAT
  (100.64.0.0/10) is not advertised. Such addresses include private, loopback, link-local,
  unspecified and multicast.
- The mapping is **kept and renewed** (one owned rule, released on close). It is not recreated
  every tick. Logged once at info.
- `HavePortMap` reports *advertised* (`mapping.is_some() && advertise`). This is a stated
  deviation, so control never sees `HavePortMap` without a type-3 endpoint.
- An external port of 0 or an address of `0.0.0.0` is never advertised in either mode
  (invariant 4).

**magicsock** (`ts_magicsock`):
- `SelfEndpointType::PortMapped`.
- `MagicSock::set_portmapped(Option<PortmapEndpoint>)` stores the address together with
  `expires_at`.
- `self_endpoints()` omits it after `expires_at`, and `rebind()` clears it synchronously (§3.7).
- The order of `self_endpoints()` is explicit priority, as in Go:
  1. PortMapped
  2. Stun
  3. Stun4LocalPort
  4. Local
- Results are **de-duplicated by address**; the first (highest-priority) type wins.

**Advertiser** (`run_advertiser`, `ts_runtime/src/direct.rs`):
- Stop sorting by `(addr, ty)`. Publish the de-duplicated priority order, and detect changes on
  `(ordered list, StunFacts, PortMapFacets)`, which gives Go's `BasicallyEqual` semantics for these
  fields.
- Wake immediately on `PortmapState` changes instead of waiting for the 5 s tick. The tick remains
  as a fallback.
- `StunFacts { working_udp: bool, mapping_varies: bool }` is computed **from the pre-dedup STUN
  set**, using the logic of today's `net_info_working_udp` / `net_info_mapping_varies`, moved
  from `ts_control/src/tokio/client.rs`.
- It is carried alongside the endpoints. A STUN address that equals the portmapped address
  therefore still counts as STUN evidence.
- A portmap-only list never sets `working_udp`.

**Control** (`ts_control`):
- `EndpointAdvertisement` gains `stun: StunFacts` and `port_map: PortMapFacets { upnp, pmp, pcp:
  Option<bool>, have_port_map: bool }`. Both types are defined in `ts_control`, with no dependency
  on `ts_portmapper`.
- `Command::SetEndpoints` carries both.
- `CarriedNetInfo` gains `upnp`, `pmp`, `pcp` and `have_port_map`, applied to **every** map
  request, reconnects included.
- `WorkingUDP` / `MappingVariesByDestIP` come from `StunFacts` rather than from endpoint types.
- `map_request_builder` gains `upnp()`, `pmp()`, `pcp()` and `have_port_map()`.
- `control_runner` maps `PortMapped` to wire `EndpointType::PortMapped` (= 3, which already exists
  in `ts_control_serde`).
- UPnP stays `None` in phases 0–1. We never probe it, so we never claim `false`.

**CallMeMaybe gate** (`run_call_me_maybe`): `PortMapped` counts as a reflexive candidate, like
`Stun`. `seal_call_me_maybe` picks up the de-duplicated list.

### 3.9 Runtime driver (`ts_runtime/src/portmap.rs`, spawned by `DirectManager::on_start`)
```
spawned unless disabled (config/env); owns Client + state receiver
loop select (biased) {
  shutdown.changed()                       => break     // release is done by Runtime (§3.12), not here
  poll every 10 s                          => client.check_network(); suspend check (§3.7); reap
  sleep_until(sched.expires_at)            => client.expire_if_due()
  sleep_until(max(renew_after, next_attempt_at)) if renew_while_idle && peers>0 && mapping
                                           => attempt()
  periodic ReSTUN tick (20–26 s jitter) if should_do_periodic_restun(..) => tick()
  kick.notified()                          => tick()   // on_start, first peer, rebind, re_stun, knob change
  self-node knobs changed                  => only_tcp_443 → client.disable / re-enable
  state_rx.changed()                       => sock.set_portmapped(advertisable(state)); wake advertiser
}
tick():
  client.set_local_port(sock.local_addr().port())   // 0 → Err, no mapping, warn once
  report = client.probe().await                       // cached 10 min positive, Go parity
  client.get_cached_mapping_or_start_creating_one()   // respects next_attempt_at; never blocks
```
- `rebind` and `rebind_and_reprobe` call `client.note_rebind(...)` **synchronously** (§3.7), then
  kick.
- `re_stun` only kicks.
- Logging is Go-equivalent in content, rate-limited by state change (standing in for
  `LinkChangeLogLimiter`), and survives release builds:
  - **info:** mapping obtained (proto, external, granted lease); renewed with a changed port;
    lost (expiry, epoch, network change, disable); released or left to expiry; probe result
    changed; disabled by knob; advertisement suppressed by policy.
  - **warn:**
    - router refusal codes;
    - malformed reply (once per network generation);
    - LAN send errors such as `EHOSTUNREACH` and `EPERM`, verbatim, once per generation;
    - ledger full;
    - lookup errors other than `Ok(None)`.
  - Per-tick traffic is `debug`/`trace` only.
  - Never log nonces, keys or full URLs.

### 3.10 Ready for UPnP (phase 2), with no code now
- `LanDest` is `#[non_exhaustive]` and will gain `SsdpUnicast`, `SsdpMulticast` (interface-scoped,
  TTL 1) and `Http { port }`.
  - Every HTTP target is `gw:<validated port>`.
  - Phase 2 must pin and revalidate `URLBase`, absolute and relative `controlURL`, and redirects.
    It forbids redirects, DNS, proxies, userinfo and non-HTTP schemes.
  - It must bound headers, decoded chunked bodies, XML depth and nodes, and service count.
- `Mapping.proto` and `ProbeReport.upnp` already exist. `OwnedRules` already models "delete by
  identity".
- **Permanent-lease policy, decided now (finding H4):**
  - Phase 2 requests finite leases.
  - The 402/725 permanent-lease fallback is **opt-in**. It is allowed only with persisted
    ownership (stable request identity in the state dir) and ownership-aware recovery before any
    retry.
  - A persisted port is never deleted blindly on a different router.
  - Lost-Add-response tests gate phase 2.
- Prefer the existing HTTP/1 transport (`ts_http_util::http1::connect` over a cap-guarded stream)
  to a new parser. The XML dependency decision stays in phase 2.

### 3.11 Config chain, knobs and runner flags
- **`tailscale::Config`** (`src/config.rs`) gains:
  - `disable_port_mapper: bool` — Go `DisablePortMapper`; default `false`, i.e. enabled.
  - `port_mapper: PortMapperOptions { gateway_override: Option<(Ipv4Addr, Option<Ipv4Addr>)>, advertise_nonpublic_external: bool /* default true */, renew_while_idle: bool /* default false */ }`.
- **Pass-through:** `ts_control::Config` (transport-only; control never reads it) → `ts_runtime`
  env (`env.rs`) → `DirectManager`.
- **Cargo:**
  - Feature `portmapper` on `ts_runtime` and the facade, default on (Go parity).
  - With the feature off, the driver is not compiled and the facets stay `None`.
  - An open question for the GeiserX PR is whether upstream wants default-off (§9).
- **Shared test construction** (the `ts_runtime` test env builders and `tests/` helpers) sets
  `disable_port_mapper = true`. A test asserts the default test config builds no client
  (case 31).
- **Runner** (`examples/exit_node`):
  - `--no-portmapper`.
  - `--portmap-gateway GW[,SELF]`.
  - `--portmap-advertise-nonpublic`: reverts (a) to Go behaviour. The runner's default is
    suppression on.
  - `--portmap-lazy-renew`: reverts (b) to Go behaviour. The runner's default is renewing while
    idle with peers.
  - Probe flags (§6).
  - `state_dir` becomes `required_unless_present = "portmap_probe"`.
  - The status line prints Go's `portMapSummary` form, for example `portmap=active-MC ext=…`.

### 3.12 Shutdown and release (finding H1)
- `Runtime` gains `portmap: Option<CleanupHandle>`. It is a sync, `Arc`-backed handle onto the
  client, so no actor `ask` is needed from `Drop`.
- **`graceful_shutdown(timeout)`.** The portmap close runs **inside** the already-timed
  `_shutdown_all` future, as its first step, concurrently with nothing else that touches the
  socket:
  - `cleanup.close(min(1 s, timeout))`;
  - then the existing control, dataplane and bus stops.
  - This stays within the caller's total timeout; the runner passes 5 s. No extra second is added
    in front.
  - Close stops new work before it awaits (§3.5.3).
- **`Drop`.** `cleanup.release_blocking_best_effort()`:
  - `Closing` → `Closed`;
  - one non-blocking `send_to` per deletable owned rule via `GatewayCap::pxp_send_blocking`;
  - no tokio runtime needed; no wait for replies;
  - skipped if already closed, if the policy forbids LAN sends, or if the network doesn't
    revalidate.
- **Release matrix:**

| Event | Release? |
|---|---|
| Close / Drop | Yes, if the policy allows LAN sends and the network revalidates |
| `set_local_port` change; protocol switch | Yes: the old rule, from the ledger |
| Network change, old network still validates | Yes |
| Network change, old network gone or unknown; network down; failed rebind with the network gone | No |
| Epoch rollback | No (Go) |
| `only-tcp-443` | No: left to router expiry; cleaned up if re-enabled on the same network |
| Stale commit (rule committed after invalidation) | Yes, under the same conditions as close |
| Expiry | No: nothing to release |
| Suppressed advertisement (a) | No: kept and renewed, released on close |

---

## 4. Router-state bounds, stated honestly (finding H4)
- **Normal state.** One UDP translation, `external:assigned → self_ip:underlay port`. It is open to
  any source host. It is not TCP and not a route. WireGuard and disco authenticate overlay
  traffic, but the listener receives unsolicited internet UDP and can be flooded; that is the same
  exposure Go accepts. Renewals update this one rule. Port changes and protocol switches keep
  cleanup ownership of the old rule (ledger).
- **After a clean shutdown,** deletes are sent best-effort and unacknowledged. A lost delete
  leaves the rule until its **granted** lifetime ends.
- **After a crash, SIGKILL or power loss,** PMP and PCP rules survive until their **granted router
  lifetime**. We request 7200 s. PMP routers cap at their own policy. PCP may legally grant
  **more** than requested (RFC 6887 §15). Our local trust is capped at 2 h, but that does not
  change the router's expiry. The bound is "granted lifetime", not "2 h".
- **Rules whose reply was lost** are in the ledger and deletable while the process lives. After a
  crash they fall under the previous point.
- **Outstanding rules per process:** at most 4 (the ledger cap).
- **DHCP reassignment:** a later holder of our LAN IP could receive traffic for a stale rule until
  it expires.
- **Probe mode** requests a **120 s** lease by default, so a crashed probe leaks at most what the
  router grants for a 120 s request.
- **Never** issue wildcard deletes (internal port 0), and never overwrite unrelated rules to
  resolve conflicts.

---

## 5. Anti-leak enforcement and CI
The real boundary is the type system plus runtime tests:
- `GatewayCap` is the only constructor of sockets. Its API takes `LanDest`, not addresses. No raw
  socket or `SocketAddr` send API is exported.
- `TestCap` is gated behind `cfg(test)` and the `test-harness` feature.
- Runtime denial tests (cases 24 and M12) check that:
  - non-gateway destinations are unconstructible;
  - a disabled policy refuses sends;
  - an invalidated cap refuses sends;
  - a bind failure fails closed.
- A **dependency/feature-graph check** (new `checks/src/portmapper_lan_only.rs`, registered in
  `CHECK_FNS`) verifies three things with `cargo metadata`:
  - `ts_portmapper` has no TLS or HTTP-client dependency;
  - `ts_control` does not depend on `ts_portmapper`;
  - `ts_runtime`'s normal features don't enable `ts_portmapper/test-harness`.
- **Tripwire scan** in the same check. It is not proof, and the doc comment says so.
  - Outside `cap.rs` and `testing/`, flag any of these patterns: `UdpSocket`, `TcpStream`,
    `TcpSocket`, `socket2::`, `libc::socket`, `libc::sendto`, `send_to(`, `connect(`,
    `Ipv6Addr::UNSPECIFIED`, `"[::]"`.
  - The gateway module's sysctl, file reads and the no-send `connect()` source-selection probe are
    allow-listed by function name.
  - It comes with adversarial fixtures: an aliased import and a qualified path, both flagged.
- Existing `ipv4_only_host_net` and `ipv4_only_forwarder` are unaffected.
- No test may contact a real gateway. The default runtime test config is asserted to never call
  the lookup (case 31). There are no `#[ignore]` live tests in phases 0–1. The live check is
  `--portmap-probe`, run manually by Claude or Kasper (§6).

---

## 6. Phase 0 (2–3 days): lookup, codecs, harness, one-shot client, probe

**Deliverables:**
1. **Gateway lookup.** §3.2: macOS sysctl with the bounds-checked parser, Linux `/proc/net/route`,
   `other.rs`, selection and validation, override validation, fixtures, the repeated-fetch test.
2. **Codecs.** §3.4: NAT-PMP and PCP with strict validation, golden tests and proptests. Fuzz
   targets: `parse_pmp`, `parse_pcp`, `parse_pcp_map`, `parse_rt_dump`, `parse_proc_net_route`.
   - Go's PCP fuzz seeds are ported, but with stronger expectations: a MAP parser must reject an
     ANNOUNCE opcode.
   - Go's golden PCP reply is ported with its public external address **rewritten** to
     `203.0.113.x`, recomputing the bytes. No public IP is committed.
3. **`GatewayCap` and `TestCap`** (§3.3), with the policy handle.
4. **Fake router harness** (`testing/`, §8.3): PMP and PCP fully implemented, plus delays,
   drops, reordering, duplicates, epoch control, commit-then-drop-reply, and two routers.
5. **Minimal one-shot client** (`oneshot.rs`):
   - `probe(cap)`, `map(cap, proto, internal, lifetime)`, `delete(cap, rule)`.
   - Single attempt, no state machine.
   - Phase 1's client calls these same functions for its I/O, so there is no second mapper.
6. **`--portmap-probe` in `examples/exit_node`:**
   - It runs **before** any `Device` or identity is constructed. It needs no state dir, keys or
     control.
   - Flags:
     - `--portmap-proto pmp|pcp|both` (default `both`; each protocol is exercised **separately**,
       one after the other).
     - `--portmap-test-port N` (default 0 = ephemeral).
     - `--portmap-lifetime SECS` (default 120, max 7200).
     - `--portmap-hold SECS` (default 0, max 600).
     - `--portmap-gateway GW[,SELF]`.
   - Steps:
     1. Bind a **dedicated test UDP socket** on `self_ip` (interface-bound). Refuse a port equal
        to `--port`, 41641 or 41642, and never use the node's socket. The request's internal port
        is the socket's *actual* nonzero port.
     2. Probe: PMP public-address request and PCP ANNOUNCE.
     3. For each protocol: MAP, report, optionally hold, DELETE, report the delete reply.
     4. Exit.
   - Hold mode keeps the mapping and test socket open. For each inbound datagram it prints the
     count, source `ip:port` and length — never the payload, never an echo — so reachability can
     be checked from an outside network.
   - Output is stable `key=value` lines:
     ```
     env os=<product version/build> uid=<n> euid=<n> exe=<path> ppid=<n> xpc_service=<$XPC_SERVICE_NAME or ->
     gateway gw=<ip> self=<ip> if=<name>/<index> prefix=/<n> source=<discovered|override> bind=<ifscope|device|source-ip>
     probe pmp=<true|false> pmp_epoch=<n> pmp_ext_ip=<ip> pcp=<true|false> pcp_result=<code> pcp_epoch=<n> errors=<verbatim>
     map proto=pmp internal=<port> requested_lifetime=<s> result=<code|ok> external=<ip:port> lease=<granted s> epoch=<n> err=<EPERM (os 1)|EHOSTUNREACH (os 65)|timeout|…>
     hold proto=pmp seconds=<n> inbound_datagrams=<n> sources=<ip:port,…>
     delete proto=pmp sent=<ok|err …> reply=<ok lifetime=0|code|timeout>
     ```
   - Exit codes: 0 if every requested protocol mapped and deleted; 2 if partial; 1 if none, or on
     a lookup or cap error.
   - Output may contain the router's public IP. It goes to stdout or logs only and is **never
     committed** (`docs/` captures must redact it).
7. **CI check** (§5): tripwire plus dependency graph, with adversarial fixtures.

**How the probe is run (later, by Claude or Kasper; not by implementers):**
- A **separate one-shot launchd system job**: its own label, not `dev.kasper.rust-exit`,
  `KeepAlive=false`, the same `UserName` and wrapper as the daemon, in the `system` domain.
- Probe each protocol separately on a dedicated test port, with a hold window for an
  outside-network reachability check from the laptop.
- Exercise:
  - a normal launch;
  - before-login boot;
  - a route-not-ready retry;
  - sleep and wake.
- Compare against the Go node under the same conditions; it is a reference, not a guarantee.
- SSDP and multicast checks are phase 2.

**Done when:**
- All phase-0 tests (§8) pass on macOS and Linux with `cargo +1.95.0`.
- `cargo fuzz` builds; each target runs ≥ 5 min clean locally.
- The checks pass.
- The probe runs against the harness in an integration test.
- Field validation is a follow-up gate *before phase 1 lands*, and **either** protocol is enough:
  1. in kmini's launch context, PMP **or** PCP returns a nonzero external port and the router
     table shows the rule;
  2. an outside client's datagram reaches the test port during hold;
  3. the delete removes the rule.

## 7. Phase 1 (5–8 days, including an overnight idle/renewal observation)

**Deliverables:**
1. Lifecycle client: §3.5 state, attempts, commits, ledger and close; §3.6 schedule and disable;
   §3.7 network identity, poll, suspend detection and probe-only epoch.
2. Runtime driver (§3.9) and its wiring into `DirectManager`:
   - `on_start` spawn;
   - synchronous invalidation in `rebind`, `rebind_and_reprobe` and the failed-rebind path;
   - `re_stun` kick;
   - `apply_self_node_knobs` handling `only-tcp-443`.
3. magicsock, advertiser and control integration (§3.8): `PortMapped`, priority and dedup,
   `StunFacts`, `PortMapFacets`, `CarriedNetInfo`, builder setters, wire type 3, the CallMeMaybe
   gate.
4. Config chain, Cargo feature, env knob, test-config isolation (§3.11).
5. Shutdown: `CleanupHandle`, `graceful_shutdown` within its timeout, the `Drop` path (§3.12).
6. Runner flags (§3.11), with the (a) and (b) defaults on in the runner and the status-line
   summary.
7. Docs and ledger:
   - `PORTING.md`: move `net/portmapper` into the mapping table (`porting_ledger` check), and
     give the NetInfo facets a target.
   - `docs/PARITY_ROADMAP.md`, `docs/MIGRATION_STATUS.md` (`tsr-x4yg`).
   - A short `docs/` note on router-state bounds (§4) and the deviations (§0.2, §1, `HavePortMap`
     semantics).

**Done when:**
- All phase-1 tests pass, including every acceptance gate in §8.4.
- Field check, after a health-checked deploy by Kasper or Claude:
  1. kmini's NetInfo shows `PMP`/`PCP`.
  2. The hard-NAT laptop gets `tailscale ping` via `<public>:<mapped>` and `tailscale status`
     direct within about 1 min.
  3. Overnight, the mapping renews at about 1 h with the runner's idle renewal, and nothing
     storms.
  4. Stopping the runner removes the router entry.
- `feat/portmapper` is rebased onto, or merged with, `ts-exit/main` for the kmini deploy. The
  GeiserX PRs are prepared against `origin/main`. Nothing is pushed by agents.

---

## 8. Test plan

Tags:
- **P0**: phase 0.
- **P1**: phase 1.
- **P2**: deferred with UPnP.
- **★**: review 36 acceptance gate (phase-1 required).
- **M#**: review 36 "missing priority" (phase-1 required).

All tests use the harness on loopback. None contacts a real gateway.

### 8.1 Go tests, and what happens to each
| Go test | Rust | Phase |
|---|---|---|
| `TestCreateOrGetMapping`, `TestClientProbe`, `TestClientProbeThenMap` (live router) | **Replaced** by the manual `--portmap-probe` field run (§6). No live cargo tests. | P0 (probe) |
| `TestProbeIntegration` | **Port** the PxP half: counters `{PMP 1, PMPPublicAddr 1, PCP 1}`, `pmp && pcp` (Rust's harness answers PMP; Go's doesn't). The UPnP half goes to P2. | P0 |
| `TestPCPIntegration` | **Port**: PCP-only harness; the probe sees only PCP; create gives a valid external; the mapping is stored (lifecycle part P1) | P0 / P1 |
| `TestUpdateEvent` | **Adapt and strengthen**: a `PortmapState` change is observed for PMP **and** PCP success, and for loss, expiry and disable. No exactly-once claim. | P1 |
| `TestGetUPnPErrorsMetric` | Ported with metrics in P2 (or N/A if metrics are skipped) | P2 |
| `TestParsePCPMapResponse` | **Port** with the golden reply's external address rewritten into 203.0.113.0/24; nonce = bytes 24..36 | P0 |
| `FuzzParsePCPResponse`, `FuzzParsePCPMapResponse` | **Port** seeds plus the invariant "never both a mapping and an error, never neither"; MAP rejects ANNOUNCE | P0 |
| `TestSelectBestService`, `TestParseUPnPDiscoResponse`, `TestGetUPnPClient`, `TestGetUPnPPortMapping*`, `TestProcessUPnPResponses`, `TestGetUPnPPortMappingNoResponses` | Port all (v1 §5.1 list). SOAP faults may arrive with HTTP 200. | P2 |
| netcheck `TestReportString` portmap cases | **Adapt**: `PortMapFacets::summary()` plus the `active-` prefix | P1 |
| netmon `TestLikelyHomeRouterIP`, `_Prefix`, `_NoMocks` | **Adapt**: self-IP selection on injected interface lists; `_NoMocks` is a smoke test that calls the real lookup (a **local read only**, no packets) | P0 |
| netmon `TestExtremelyLongProcNetRoute` | **Port** (line and byte cap) | P0 |
| netmon `TestFetchRoutingTable` | **Port** the repeated-fetch regression (10 fetches) | P0 |
| netmon `TestLikelyHomeRouterIPSyscallExec` | **Adapt** (macOS): compare with `route -n get default` (exec of a local tool, no network) | P0 |
| netmon default-route-interface tests (cloud run, netlink) | N/A | — |
| magicsock tests with `DisablePortMapper: true` | **Adapt**: the shared runtime test config disables it; case 31 | P1 |

### 8.2 Cases (v1 numbering kept; revised per review 36)
Codecs and lookup:
1. (P0) PMP build golden: address request, MAP, and delete with lifetime 0 and external port 0;
   internal port 0 is unconstructible.
2. (P0) PMP parse:
   - wrong version or opcode rejected; wrong success length rejected;
   - `0.0.0.0` address rejected;
   - result codes 1–5 and an unknown code are typed errors;
   - MAP internal-port mismatch rejected; external port 0 with lifetime > 0 rejected.
3. (P0) PCP:
   - golden ANNOUNCE, MAP and delete (Errata 3621, original nonce);
   - R bit, opcode, protocol 17, internal port and nonce all validated;
   - short header-only error responses carry their lifetime;
   - not-IPv4-mapped external address rejected; length not a multiple of 4 or > 1100 rejected.
4. (P0) Lifetimes 0, 1 and `u32::MAX` don't panic. Lifetime 0 on create is a failure.
32. (P0) macOS route-dump fixtures (§3.2 list); truncated or garbled input → `Parse`; offsets
    asserted against `libc`.
33. (P0) Linux fixtures: public best default → `NotPrivate` (no fall-through), metric selection,
    tie → `Ambiguous`, down route, hex order, cap.
34. (P0) Fuzz targets as in §6. The UPnP targets are P2.
14. (P0) IPv6-only default → `GatewayIpv6`; no packet sent.
15. (P0) Non-private, not-on-link, point-to-point or vanished interface → typed error; no packet
    sent.
- L1. (P0) Self-IP selection: an `RTAX_IFA` preference, the source-selection cross-check for
  multiple candidates, a disagreement → `Ambiguous`, the prefix must contain the gateway.
- L2. (P0) The override goes through production validation (a public override is rejected).
  `TestCap` can't be built outside `cfg(test)` / `test-harness` (compile-fail doc test).

One-shot client and probe (P0):
- 20a. Hostile replies at the codec and cap level: truncated, wrong version or opcode, garbage,
  oversize (the cap+1 buffer detects truncation), wrong source IP or port, a PCP wrong nonce
  followed by a correct reply (accepted).
- 20b. A burst of 1000 junk datagrams does not extend the absolute deadline. This is asserted on
  computed deadlines with a manual clock, plus a loose wall-clock bound (< 2 s) to stay robust
  on CI.
21. No responder: the probe returns all false within the exchange budget (same assertion style
    as 20b). The one-shot map gives `NoServices`.
24a. `GatewayCap`:
    - only `LanDest::Pxp` exists;
    - a send after `invalidate()` or with the policy off is refused without a socket write
      (harness counter 0);
    - a bind to a missing `self_ip` fails closed.
- P1p. The probe command against the harness:
  - test port ≠ node port (refuses 41641, 41642 and `--port`);
  - an ephemeral bind gives a nonzero internal port;
  - each protocol is mapped and deleted separately;
  - the delete reaches the harness with the right identity;
  - hold mode reports inbound datagram counts without payloads;
  - it runs with no state dir;
  - the output keys match §6;
  - errors (injected `EHOSTUNREACH` / `EPERM` from `TestCap`) are printed verbatim.

Lifecycle (P1; manual lease clock plus harness):

5. Protocol-selection matrix over {PMP, PCP seen} × {PMP or PCP disabled}: `preferPCP` parity;
   neither seen → `NoServices` plus backoff (no UPnP in P1).
6. Cache: a call before `renew_after` sends nothing.
7. ★ Renewal:
   - past `trusted/2`, the request carries `prev_ext_port`; PCP reuses the nonce;
   - a router-assigned new port updates the endpoint and publishes the state;
   - an unchanged external address on renewal causes no republish to control;
   - **an initial mapping with external ≠ internal port flows through to the advertisement and
     to the delete.**
8. Expiry: past `expires_at`, `have_mapping()` is false, the endpoint is withdrawn, and the next
   call starts a create (subject to `next_attempt_at`).
9. ★ Single-flight: 50 concurrent `get_cached…` calls → exactly one MAP. Single-flight resets after
   success, error, panic in the attempt future, and abort. The `JoinSet` is reaped.
10. Probe cache: no PxP traffic within 10 min after "seen"; traffic again after invalidation.
11. Epoch, probe-only:
    - backwards → drop without release, cache cleared;
    - equal or higher → keep; 0 → ignored; the same for PCP;
    - a stale probe's epoch can't drop a newer mapping;
    - reboot scenario: the harness resets the table, **then advances the epoch to a small
      nonzero value** and the positive cache is expired before detection is expected (Go's
      zero-ignore rule).
12. ★ Network change:
    - a new gateway or self IP: the old rule is released **to the old router only** (two
      harness routers, the second receives nothing for the old rule), and the new mapping goes
      via the new router;
    - an identity change where the old network no longer revalidates: **no delete is sent**
      anywhere.
13. Lookup failure or `Ok(None)` → invalidate like network-down, no release.
16. ★ `set_local_port`:
    - same port → no-op;
    - a new port → the old rule is released, and the new MAP carries the new internal port;
    - **port 0 → `LocalPortZero`, mapping invalidated, and no MAP ever carries internal 0.**
17. `note_network_down`: invalidate without release.
18. ★ Close:
    - PMP delete with the right internal port and external port 0; PCP delete with lifetime 0
      and the nonce;
    - idempotent;
    - close stops new work *before* awaiting;
    - it finishes within its budget with the router gone;
    - `graceful_shutdown(Some(t))` completes within `t`, with no added second;
    - the `CleanupHandle` Drop path sends the deletes with no tokio runtime and no reply wait;
    - it does nothing after close.
19. Router errors:
    - PMP MAP codes 1–5: no mapping, one warn, retry follows backoff (no hot loop);
    - PCP ANNOUNCE `NOT_AUTHORIZED` / `ADDRESS_MISMATCH` → `pcp=false`;
    - PCP MAP non-OK → no mapping, `next_attempt_at` honours the error lifetime;
    - PMP code 2 and PCP `NOT_AUTHORIZED` → ≥ 30 min.
20. ★ Hostile replies through the full lifecycle:
    - everything in 20a/20b, in create, renew and delete;
    - **wrong protocol, wrong internal port, missing R bit, short error responses**;
    - a duplicate of an earlier success arriving after a renewal does not roll back state.
22. Advertising policy (a), both settings:
    - Go default: a private or CGNAT external is advertised;
    - runner setting: it is not advertised, `HavePortMap` stays false, the mapping is retained,
      renewed (not recreated: harness create count stays 1 across ticks) and released on close;
    - `0.0.0.0` or port 0 is never advertised in either mode.
23. ★ Disable:
    - config or `TS_DISABLE_PORTMAPPER` (injected env reader): no client, no socket, zero harness
      packets, facets `None`;
    - `only-tcp-443` live: in-flight attempt aborted, the next send refused at the cap, mapping
      withdrawn, **no delete sent**, facets `Some(false)`;
    - on re-enable (same network) leftover rules are deleted first.
24. ★ LAN isolation:
    - 24a, plus the PCP and delete packets' source address equals `self_ip` in source-consistent
      loopback mode (M7);
    - the dependency-graph check;
    - adversarial tripwire fixtures.
25. Logging (`tracing-test`):
    - obtained, renewed-with-new-port, lost, released and left-to-expiry lines at INFO;
    - 100 identical failing ticks → one WARN;
    - no nonce in any log line.

Integration in the Rust tree (P1):

26. `self_endpoints()` priority order and de-duplication; the CallMeMaybe gate works with only a
    portmapped endpoint; `seal_call_me_maybe` includes it.
27. ★ Wire integration:
    - `control_runner` maps `PortMapped` to `3` (JSON golden);
    - `CarriedNetInfo` carries `PMP`, `PCP` and `HavePortMap` on **every** request, reconnect
      included;
    - UPnP stays unset;
    - `WorkingUDP` does not become true from a portmap-only list.
28. Advertiser: a change only in `PortMapFacets` or `StunFacts` republishes; an unchanged one
    doesn't; a state change wakes it before the 5 s tick.
29. `rebind_and_reprobe` with a port change → synchronous invalidation, release, new mapping;
    same port and network → mapping restored with no I/O.
30. ★ Runtime end-to-end, with the harness behind a `TestCap`-backed gateway injected through the
    test-harness feature:
    - a mapping is obtained;
    - the type-3 endpoint is advertised to a fake control;
    - `graceful_shutdown` makes the harness see the delete within the timeout.
31. Default runtime test config: the lookup is injected as a panicking stub and never called;
    no client is built. The same holds under feature combinations: `portmapper` off, on, and
    with `test-harness`.

Review 36 missing priorities (all P1, all required):
- M1. A delayed success landing during a simultaneous renewal, rebind, close or disable → the
  stale commit is rejected, the rule is `Orphaned` and cleaned up per §3.5.2, and the newer state
  wins.
- M2. Cancellation after router commit (harness commit-then-drop-reply, and abort after send) →
  the ledger holds the rule; close or the next cleanup deletes it.
- M3. Expired endpoint removal while idle **and** while peerless: no ReSTUN ticks, and the
  endpoint is still withdrawn at `expires_at`.
- M4. A failed renewal with a past `renew_after`: attempts are spaced by backoff (count MAPs over
  10 simulated minutes), with no storm from repeated kicks.
- M5. Success and loss notifications for both PMP and PCP (see the `TestUpdateEvent` row).
- M6. A STUN address identical to the portmapped address: one advertised endpoint (PortMapped),
  `WorkingUDP` still true, `mapping_varies` unchanged.
- M7. Source-bound PCP and delete packets (source equals `self_ip`); the harness enforces the PCP
  client IP and returns `ADDRESS_MISMATCH` otherwise.
- M8. PCP protocol, internal port and R-bit validation, and short error responses (also in 3 and
  20).
- M9. Oversized-datagram truncation detection (cap+1).
- M10. Finite-lease success: the granted lifetime differs from the requested one (shorter and
  longer); the schedule uses `min(granted, 2 h)` and `router_expiry` records the granted value.
- M11. Wrong-network delete suppression (case 12, second part).
- M12. Wall-clock jump and suspend/resume: an injected `SystemTime` vs `Instant` discrepancy
  triggers re-check and renewal; a false positive costs exactly one renewal.
- M13. Ledger bound: with the router never replying, outstanding rules never exceed 4, and new
  attempts are refused with one warn.
- M14. A delete is scoped to our internal port: two clients with the same self IP and different
  internal ports; deleting one leaves the other (co-resident Go `tailscaled`).
- M15. The renew-while-idle policy (b), both settings:
  - lazy: no renewal while idle;
  - runner: a renewal at `renew_after` while idle with peers, and no renewal with zero peers
    (it expires).
- M16. Network poll: the identity changes while idle (no ReSTUN) → detected within one 10 s poll.
- M17. Harness shutdown is awaited explicitly; no task or log after `shutdown().await`.

UPnP cases 35–41 from v1, review 36's lost-permanent-Add test, and `TS_DISABLE_UPNP` /
`debug-disable-upnp`: **P2.**

### 8.3 Fake router harness (`ts_portmapper/src/testing/`, P0)
- **Routers.** `FakeRouter::start(opts).await`:
  - a PxP UDP listener on 127.0.0.1:0, with its own **independent** request decoder written from
    the RFCs (it does not reuse `pmp.rs` / `pcp.rs`);
  - full PMP and PCP: address and ANNOUNCE replies, MAP, renew, delete, a mapping table keyed by
    `(client ip, proto, internal port)`, and PCP nonce ownership checks.
- **Clock.** The table expires on the **same manual lease clock** as the client.
- **Two routers, one protocol port.** `TestCap` maps logical gateways (`192.168.1.1:5351` and
  `10.0.0.1:5351`) to each router's loopback port. Replies are presented as coming from the
  logical `gw:5351`. On Linux, an extra variant binds the routers to `127.0.0.2:5351`-style
  addresses for real. macOS has only 127.0.0.1 without root, so that variant is Linux-only.
- **Source-consistent loopback mode.** The client binds 127.0.0.1 and the PCP client IP is
  127.0.0.1. The "inconsistent" mode uses self `198.51.100.4` in place of Go's arbitrary public self address.
- **Behaviour knobs, changeable at runtime:**
  - enable PMP and PCP separately;
  - `public_ip` (default 203.0.113.7);
  - epoch: fixed, auto-advancing, `reboot()` (table cleared, epoch reset), `set_epoch(n)`;
  - per-opcode result-code overrides, including PCP error lifetimes;
  - lifetime override (shorter or longer than requested);
  - `assign_port` function (default: the requested port, else 4242, ≠ internal when configured);
  - `delay`, `drop_next(n)`, `duplicate_next(n)`, reordering (hold a reply and release it after
    the next one);
  - **commit-then-drop-reply**;
  - `reply_from_wrong_port` / `reply_from_wrong_ip`;
  - corruption: `Truncate | BadVersion | WrongOpcode | NoRBit | WrongNonce | WrongProto | WrongInternalPort | Garbage | Oversize`;
  - `enforce_pcp_client_ip`.
- **Request barriers.** `router.barrier(kind)` pauses the router before replying to the next
  request of that kind until the test releases it. This drives the race tests (M1, M2).
- **Observation:**
  - `counters()` using Go's names where they exist (`num_pmp_recv`,
    `num_pmp_public_addr_recv`, `num_pcp_recv`, `num_pcp_disco_recv`, `num_pcp_map_recv`,
    `num_pcp_other_recv`, `num_pmp_bogus_recv`, `invalid_pcp_map_pkt`) plus `num_pmp_map_recv`
    and `num_deletes`;
  - `requests()` (decoded, with source);
  - `mappings()` (live table with owners and nonces), so tests assert **rule ownership and
    counts**, not just packets.
- **Lifecycle.** `shutdown().await` stops and joins every task. `Drop` aborts as a backstop
  only. After shutdown there are no late logs.
- Exposed to `ts_runtime` tests through the `test-harness` feature (dev-dependency only).

### 8.4 Phase-1 acceptance gates
Cases **7, 9, 12, 16, 18, 20, 23, 24, 27, 30** (★) and **M1–M17** must pass. So must the full P0
suite, `cargo clippy` (strict, as the repo configures it), `cargo fmt`, the `checks` binary, and
the fuzz smoke runs. All of it runs with `RUST_TEST_THREADS=4` and `-j4`.

---

## 9. Risks and open questions
1. **macOS Local Network privacy.** TN3179 exempts launchd daemons, and kmini's job is a system
   daemon even with `UserName`. That is an inference, not a measurement. The phase-0 probe, run
   in the same launch context, settles it for PxP unicast. This session's own shell saw SSDP
   multicast fail with "No route to host" while unicast to the router worked. That matters for
   phase 2 SSDP, not PxP. The probe prints `EPERM` / `EHOSTUNREACH` verbatim.
2. **Default on.** The library default is on (Go parity), so ts-exit gets the mapper. Whether
   GeiserX upstream wants default-off is asked in the PR. A zero-I/O off switch exists in both
   config and env.
3. **Gateway lookup on macOS** uses a hand-written route-dump parser (Go uses `x/net/route`).
   Mitigations:
   - bounds-checked parsing with no struct casts;
   - offsets asserted against `libc`;
   - fixtures and fuzzing;
   - a cross-check against `route -n get default`;
   - the validated override.
4. **Linux main table only.** Policy-routed hosts may need the override, or a later netlink
   backend.
5. **Phases 0–1 suffice for kmini only conditionally.** A PMP or PCP mapping must actually be
   reachable from the café/laptop path. `MappingVariesByDestIP` describes mapping, not filtering.
   Today's Go comparison is strong evidence, and the phase-0 hold test confirms it for our
   binary.
6. **Endpoint churn.** A router that changes the port on every renewal makes the endpoint flap
   hourly (Go has the same behaviour). Watch the `SetEndpoints` rate overnight.
7. **Proxy-egress deployments.** A portmapped endpoint exposes the host's public IP to tailnet
   peers, as STUN endpoints do. It can add exposure where STUN is unavailable. `exit_proxy`
   deployments should consider `--no-portmapper`. We don't promise general host anonymity.
8. **Branch base.** `feat/portmapper` is based on `origin/main`. kmini runs `ts-exit/main`, so the
   phase-1 deploy needs a rebase or merge. That is plain integration work; the code touches no
   ts-exit-only files.

## 10. Commit plan (small, reviewable; each signed off, with Co-Authored-By)
Phase 0:
1. Crate skeleton, `other.rs`, workspace wiring.
2. PMP codec plus tests.
3. PCP codec plus tests (rewritten golden reply).
4. Fuzz targets.
5. Gateway types, selection and validation.
6. Linux parser plus fixtures.
7. macOS fetch and parser plus fixtures.
8. `GatewayCap` / `TestCap` plus policy.
9. Fake router harness.
10. One-shot client.
11. `--portmap-probe`.
12. `checks` tripwire plus dependency graph.

Phase 1:
1. Schedule (pure).
2. Owned-rule ledger.
3. Lifecycle client plus race tests.
4. Driver.
5. magicsock `PortMapped` plus dedup.
6. `StunFacts` / `PortMapFacets` plus advertiser.
7. Control NetInfo plus wire type.
8. Config, env, feature, test isolation.
9. Shutdown and cleanup handle.
10. Runner flags plus status.
11. Docs and ledger.

Each phase becomes one GeiserX PR, prepared locally and never pushed by agents.
