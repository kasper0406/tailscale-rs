# NAT port mapper (NAT-PMP / PCP / UPnP-IGD) for the Rust node: plan

Status: plan, not implemented. Bead: `tsr-x4yg` ("magicsock: no portmapper ... direct paths behind
mapping-NATs stay on DERP"). Go reference: `repos/tailscale-go/net/portmapper` and its users.
Rust tree: `repos/geiserx-tailscale-rs`, branch `ts-exit/main`.

## 0. Why, in one paragraph

kmini sits behind an easy NAT whose router does UPnP, NAT-PMP and PCP. A café client behind an
endpoint-dependent (hard) NAT can't reach kmini's STUN endpoint: our router filters inbound
packets by remote address and port, and kmini can't predict the café NAT's port. Go gets
direct anyway by asking the router for a real inbound mapping (`external:port -> mac:41642`)
and advertising it as an `EndpointPortmapped` (type 3) endpoint. That port is open to any
source, so the café client's ping gets through. The Rust node has no port mapper
(`PORTING.md:121`, `docs/MIGRATION_STATUS.md:33`).

---

## 1. How Go's portmapper works

Refs are `net/portmapper/<file>:<line>` unless noted.

### 1.1 Objects and state
- `Client` (`portmapper.go:113-156`), guarded by one mutex. Fields: `localPort` (magicsock's UDP
  port), `lastGW` / `lastMyIP`, `runningCreate` (single-flight flag), `lastProbe`, per-protocol
  "seen" caches (`pmpPubIP` / `pmpPubIPTime` / `pmpLastEpoch`, `pcpSawTime` / `pcpLastEpoch`,
  `uPnPSawTime` / `uPnPMetas`), and at most one current `mapping`.
- `mapping` interface (`portmapper.go:171-187`): `Release(ctx)`, `GoodUntil`, `RenewAfter`,
  `External`, `MappingType` ("pmp" / "pcp" / "upnp"). Implementations: `pmpMapping`
  (`portmapper.go:198-234`), `pcpMapping` (`pcp.go:53-92`), `upnpMapping` (`upnp.go:75-115`).
  Mappings are immutable after creation.
- Public API (`portmappertype/portmappertype.go:44-79`): `Probe`, `HaveMapping`,
  `SetGatewayLookupFunc`, `NoteNetworkDown`, `GetCachedMappingOrStartCreatingOne`,
  `SetLocalPort`, `Close`. A mapping event `{External, Type, GoodUntil}` is published on the
  event bus (`portmapper.go:533-542`). magicsock subscribes and calls
  `ReSTUN("portmap-changed")` (`wgengine/magicsock/magicsock.go:3699`).

### 1.2 Timeouts and caches
| What | Value | Where |
|---|---|---|
| Wait for a PMP/PCP/UPnP UDP reply | 250 ms | `portmapper.go:106`, `:692`, `:918` |
| Trust "service seen" (skip re-probing it) | 10 min | `portmapper.go:110`, `:416-440` |
| Skip PxP and go straight to UPnP if a probe < 5 s ago saw neither PMP nor PCP | 5 s | `portmapper.go:665-673` |
| Whole background create attempt | 5 s | `portmapper.go:510` |
| Requested lease (PMP, PCP and UPnP) | 7200 s; renew at lease/2 | `portmapper.go:814`, `:764-767`; `pcp.go:30`, `:149-156`; `upnp.go:567-569` |
| Extra wait after the first UDP reply to collect more UPnP replies | 50 ms | `portmapper.go:1011-1019`, `:1067-1069` |
| Maximum UPnP replies kept | 10 | `portmapper.go:1091` |
| UPnP root-description fetch | 1 s; HTTP idle timeout 2 s | `upnp.go:278`, `:449` |

There is no retransmission and no exponential backoff. Rate limiting comes from single-flight
(`runningCreate`, `portmapper.go:502-507`), the "seen recently" caches, and magicsock's ReSTUN
cadence. Logs go through `netmon.LinkChangeLogLimiter`, which prints each distinct line once
per link change (`magicsock.go:682-683`).

### 1.3 Gateway and own IP
- `gatewayAndSelfIP()` (`portmapper.go:331-346`) calls the lookup function. If the gateway or own
  IP changed, or the lookup failed, it **invalidates and releases** the current mapping.
- Production lookup: `netMon.GatewayAndSelfIP` (`feature/portmapper/portmapper.go:24-41`;
  `net/netmon/netmon.go:441-465`), which caches `LikelyHomeRouterIP()` until netmon reports a
  change.
- `LikelyHomeRouterIP` (`net/netmon/state.go:703-760`):
  - macOS/BSD: dumps the routing table (`interfaces_darwin.go:20-25`) and takes the
    default-route gateway with `RTF_GATEWAY` set and `RTF_IFSCOPE` clear, plus the interface
    address (`interfaces_bsd.go:74-104`, `:113-140`).
  - Linux: reads `/proc/net/route` and requires a **private** gateway, capped at a maximum
    number of lines (`interfaces_linux.go:44-110`).
  - Fallback for own IP: the first private IPv4 on an up interface whose prefix contains the
    gateway.
- An IPv6 gateway returns `ErrGatewayIPv6` (`portmapper.go:564-566`); a failed lookup returns
  `ErrGatewayRange`.

### 1.4 Probe (`portmapper.go:896-1183`)
1. If disabled (`TS_DISABLE_PORTMAPPER`, or `DebugKnobs.DisableAll`, which is wired to control's
   `only-tcp-443`), return `ErrPortMappingDisabled`.
2. Open one ephemeral `udp4` socket. For each service: if it was seen recently, report true from
   the cache; otherwise send a probe (unless that service is disabled):
   - PMP: public-address request `{0,0}` to `gw:5351` (`:929-934`).
   - PCP: ANNOUNCE (24-byte header carrying own IP) to `gw:5351` (`:935-940`; `pcp.go:163-171`).
   - UPnP: SSDP `ssdp:all` unicast to `gw:1900` first (to open host-firewall state), then
     multicast `239.255.255.250:1900`, then an `InternetGatewayDevice:1` search to multicast
     (`:941-993`, `:1243-1256`).
3. Receive until 250 ms pass, or until PCP, PMP and UPnP have all answered and the 50 ms UPnP
   window has closed.
   - From port 1900, or from any other port if the payload contains `:InternetGatewayDevice:`
     (issue 7377): parse the SSDP reply (`upnp.go:769-779`) and keep up to 10. A reply from an IP
     other than the gateway is accepted and logged (`:1076-1079`).
   - From the PxP port, PCP ANNOUNCE reply:
     - Check the epoch first (`maybeInvalidatePCPMappingLocked`), then record `pcpSawTime` and
       the epoch.
     - Result OK sets PCP true. `NOT_AUTHORIZED` (2) and `ADDRESS_MISMATCH` (12) set PCP false.
     - Any other result is logged as unexpected (`:1113-1146`).
   - From the PxP port, PMP reply:
     - Only opcode `0x80` (public-address reply) counts. On OK: check the epoch, then store the
       public IP, its timestamp and the epoch.
     - Codes 2, 3 and 4 are logged (`:1147-1180`).
4. Afterwards, sort and de-duplicate the UPnP replies (`processUPnPResponses`, `upnp.go:704-734`:
   USN reverse-sorted so `IGD:2` comes before `IGD:1`, compacted on Location+Server), then store
   them as `uPnPMetas`. Set `lastProbe`.

### 1.5 Epoch (router restart) handling (`portmapper.go:1185-1234`)
These checks run **only on probe replies**. If the new epoch is 0, or there is no mapping of that
protocol, do nothing. If the epoch is ≥ the stored mapping's epoch, it's fine. If it went
**backwards**, drop the mapping without releasing it and clear that protocol's cache, so the next
`GetCached...` recreates it. Go leaves RFC 6887 §8.5 (time-based epoch validation) as a TODO.

### 1.6 Creating and renewing (`createOrGetMapping`, `portmapper.go:553-805`)
1. Disabled → `NoMappingError{ErrPortMappingDisabled}`. All three services disabled →
   `ErrNoPortMappingServices`. Look up gateway and own IP (§1.3).
2. An existing mapping still before `RenewAfter` → reuse it (`:626-631`). Otherwise remember
   `prevPort = mapping.External().Port()` and ask for the same external port again.
3. PCP and PMP both disabled → UPnP only (`:636-643`). A probe in the last 5 s that saw neither
   → UPnP only (`:665-673`).
4. PCP nonce: reuse the current PCP mapping's nonce (so the router treats it as a renewal);
   otherwise use 12 random bytes (`:675-682`).
5. `preferPCP = !DisablePCP && (DisablePMP || (!recentPMP && recentPCP))` (`:697`). So **PMP is
   the default** whenever PMP has been seen.
   - PCP: one MAP request (`pcp.go:99-124`), suggested external address `0.0.0.0` and the
     suggested port set to `prevPort`.
   - PMP: if the public IP isn't cached yet, send the public-address request and then a MAP UDP
     request `{internal = localPort, suggested external = prevPort, lifetime 7200}`
     (`:830-839`).
6. Receive loop, accepting only packets whose source is `gw:pxpPort` (`:730-804`):
   - **PMP:** parse (`:855-888`; the MAP reply must be 16 bytes and the address reply 12; an
     all-zero address is treated as invalid). A non-zero result returns
     `NoMappingError("PMP response Op=..,Res=..")`. The address reply fills in the external IP;
     the MAP reply fills in the external port, `goodUntil`, `renewAfter = lifetime/2` and the
     epoch. The mapping is stored once both IP and port are known.
   - **PCP:** the nonce must match bytes 24..36 of the reply, otherwise it's ignored (`:775-778`).
     `parsePCPMapResponse` (`pcp.go:128-160`): length ≥ 60; `NOT_AUTHORIZED` or any non-OK
     result → error, which becomes `NoMappingError`. External port is bytes 42..44, external IP
     bytes 44..60 (IPv4-mapped).
   - Unknown version → `NoMappingError`.
   - Read error or timeout → fall back to UPnP (`:732-742`).
   - A send error that is really a lost datagram becomes `NoMappingError`
     (`neterror.TreatAsLostUDP`).
7. `GetCachedMappingOrStartCreatingOne` (`:480-497`) never blocks:
   - valid mapping → return it, and start a background renewal if past `RenewAfter`;
   - no valid mapping → start a background create (single-flight) and return none.
   - On success the background task publishes the mapping event (`:509-543`).

### 1.7 Releasing (where Go does it)
- `Close()` invalidates **with** release (`:303-317`).
- `SetLocalPort(new)` invalidates **with** release (`:321-329`).
- A gateway or own-IP change, or a failed lookup, invalidates **with** release (`:340-344`); the
  release goes to the mapping's stored old gateway.
- `NoteNetworkDown()` and an epoch decrease invalidate **without** release (`:297-301`,
  `:1185-1234`).
- PMP release: MAP with lifetime 0 (`:226-234`).
- PCP release: MAP with lifetime 0, port 0, the all-zero address of the same family, and the
  original nonce (RFC 6887 §15.1, Errata 3621; `pcp.go:76-92`).
- UPnP release: `DeletePortMapping("", extPort, "UDP")` (`upnp.go:113-115`).

All releases are fire-and-forget.

### 1.8 UPnP-IGD (`upnp.go`, `legacy_upnp.go`)
- Disabled by `TS_DISABLE_UPNP` or the `DisableUPnP` knob (`upnp.go:460`, `:472`). The
  `DisableUPnP` knob comes from control's node attribute `debug-disable-upnp`
  (`tailcfg/nodecap/nodecap.go:124-129`, `control/controlknobs/controlknobs.go:22-23`, `:161`,
  `:196`, `magicsock.go:684-687`).
- Candidate list: the current mapping's root device first, then each stored SSDP reply
  (`:482-514`).
- Root description: parse `Location` as an IP:port URL; a host that isn't an IP is an error. If
  the host isn't the gateway, **repoint it to the gateway** (issue 5502); then GET with a 1 s
  timeout (`:246-287`).
- `selectBestService` (`:297-432`):
  - Order the clients WANIPConnection2, then WANIPConnection1, then WANPPPConnection1, then the
    legacy `urn:dslforum-org` PPP and IP services.
  - 0 clients → nil. Exactly 1 → return it without network calls (`method=single`).
  - Several:
    - For each client, call `GetStatusInfo`; "Connected" or "Up" counts as up.
    - For each up client, call `GetExternalIPAddress`. The first one with a parseable,
      non-private IP wins (`ext-public`).
    - Otherwise take the first up client with a private external IP (`ext-private`), then the
      first up client (`up`), then the first client (`none`).
- `tryUPnPPortmapWithDevice` (`:596-696`):
  - Get the external IP **before** adding the mapping (MikroTik permanent-lease leak fix).
    Reject `0.0.0.0` and loopback.
  - `addAnyPortMapping` (`:175-236`): if the external port is < 1024, pick a random port in
    1024..65535 (**never 0**: an external port of 0 is a wildcard that forwards all ports, the
    TS-2023-006 class of bug). Use `AddAnyPortMapping` on WANIPConnection2 and honour the port
    the router returns; otherwise `AddPortMapping`.
  - Request fields: remote host empty, protocol `"UDP"` (upper case is required, issue 7377),
    description `"tailscale-portmap"`, lease 7200.
  - SOAP fault `UPnPError` 725 (OnlyPermanentLeasesSupported) or 402 (Invalid Args) → retry
    once with lease 0, i.e. permanent (`:666-690`, `:739-757`).
  - Even a permanent lease is treated as `goodUntil = now + 2h` so it gets re-checked.
- The legacy `dslforum` services are the same SOAP calls under a different URN, with the field
  `NewUpTime` spelled with a capital T (`legacy_upnp.go:17-20`, `:146`, `:285`).

### 1.9 Privacy and safety behaviours to keep
- Only talk to the default-route gateway, plus SSDP multicast.
- The UPnP HTTP host is pinned to the gateway.
- PCP nonce checking.
- External port is never 0 or below 1024 for UPnP.
- External IP is validated before mapping.
- Lease ≤ 2 h, renewed at half.
- Mapping released on shutdown.
- IPv6 gateways are refused.
- Knobs: `TS_DISABLE_PORTMAPPER`, `TS_DISABLE_UPNP`, magicsock `Options.DisablePortMapper`
  (`magicsock.go:507-509`), control `debug-disable-upnp`, and control `only-tcp-443` (disables
  everything).

---

## 2. How Go integrates it

- **Construction:** magicsock creates the client unless `DisablePortMapper` is set
  (`magicsock.go:681-697`), wiring `DisableUPnP` from control knobs and `DisableAll` from
  `onlyTCP443`. The same client is handed to netcheck (`magicsock.go:710-716`).
- **Probing:** each netcheck runs `PortMapper.Probe` concurrently with STUN and waits for it
  (`net/netcheck/netcheck.go:750-771`, `:934-937`, `:994-996`). It sets `UPnP`, `PMP` and `PCP`
  to false first, so an error or a disabled mapper reports `false`, not "unknown". Netcheck runs
  on every ReSTUN (`updateNetInfo`).
- **NetInfo:** `UPnP`, `PMP` and `PCP` come from the report. `HavePortMap` comes from
  `HaveMapping()` (`magicsock.go:1055-1064`); `setNetInfoHavePortMap` flips it when the
  portmapped endpoint is added (`:1009-1023`). Sending is de-duplicated by `BasicallyEqual`,
  which includes all four fields (`tailcfg/tailcfg.go:1202-1221`).
- **Endpoints (`determineEndpoints`, `magicsock.go:1278-1345`):**
  `GetCachedMappingOrStartCreatingOne` is called before netcheck and again after. If a mapping
  exists it is added **first** as `EndpointPortmapped` (= 3, `tailcfg.go:1393-1398`), then the
  STUN endpoints, then `STUN4LocalPort`, and so on. The endpoint tracker caches up to 6 per IP,
  including the portmapped one (`endpoint_tracker.go:20-40`). CallMeMaybe carries the same list.
- **When a mapping is requested:** on every ReSTUN, i.e. link change, the periodic 20-26 s
  timer, a new peer, a key change, or a portmap change. The periodic timer only runs while there
  are peers and the node isn't idle for more than 45 s (`magicsock.go:3668-3695`). **Renewal is
  therefore lazy:** an idle node doesn't renew.
- **Network change:** `linkChange` calls `SetNetworkUp`; when the network goes down that calls
  `NoteNetworkDown` (`magicsock.go:2734-2752`). A major change triggers `Rebind()`, which calls
  `SetLocalPort(port)` (`magicsock.go:3866-3876`), followed by `ReSTUN`
  (`wgengine/userspace.go:1155-1222`).
- **Close:** `Conn.Close` calls `portMapper.Close()`, which releases (`magicsock.go:3608-3609`).
- **Debugging:** `tailscale debug portmap` (`feature/debugportmapper`) forces a type (pmp, pcp or
  upnp), runs for a set duration, and prints verbose and HTTP logs. Integration tests set
  `TS_DISABLE_PORTMAPPER=1` (`tstest/integration/integration.go:954`), and magicsock tests set
  `DisablePortMapper: true`.

---

## 3. Mapping onto the Rust tree

### 3.1 What exists today
- Single underlay socket, `0.0.0.0:<port>`; kmini pins `--port 41642`
  (`examples/exit_node/main.rs:56`, `:178`).
- Reflexive addresses come from pong harvesting plus a STUN sweep on that same socket
  (`ts_runtime/src/direct.rs:744-785`, jittered 20-26 s, gated by
  `should_do_periodic_restun`, `:848-865`).
- `ts_netcheck` deliberately does HTTPS only and has no second socket
  (`ts_netcheck/src/lib.rs:5-11`).
- Endpoints: `MagicSock::self_endpoints` (`ts_magicsock/src/sock.rs:914-1019`) returns
  `Local`, `Stun` and `Stun4LocalPort` (`ts_magicsock/src/endpoint.rs:13-25`). These are polled
  every 5 s and published as `EndpointAdvertisement` (`direct.rs:971-1002`), mapped to wire
  types in `ts_runtime/src/control_runner.rs:2147-2170`, and sent by `client.set_endpoints`. The
  wire enum already has `PortMapped = 3` (`ts_control_serde/src/netmap.rs:172-190`).
- NetInfo: `ts_control_serde/src/net_info.rs:67-79` already has `have_port_map`, `upnp`, `pmp`
  and `pcp`. The control client's `CarriedNetInfo` carries only four facets
  (`ts_control/src/tokio/client.rs:42-73`); `SetEndpoints` derives two of them (`:901-922`,
  `:534-561`). The builder has setters for those four only (`ts_control/src/map_request_builder.rs:78-102`).
- Rebind: `DirectManager::rebind` and `rebind_and_reprobe` (`direct.rs:384-445`), and
  `MagicSock::rebind`, which clears reflexive addresses and best paths (`sock.rs:700-765`).
  **There is no OS link monitor:** `NetmonSupervisor` runs against `NoopLinkMonitor`
  (`ts_runtime/src/lib.rs:455-462`). Network changes are therefore seen only through
  `Device::rebind` or by polling.
- Control knobs are read off the self node: `only_tcp_443()` and `force_background_stun()` in
  `ts_control/src/node.rs:677-693` and `:816-833`, applied in `direct.rs:1362-1383`.
- Shutdown: `Runtime::graceful_shutdown` (`ts_runtime/src/lib.rs:1475-1502`) flips `shutdown`
  first, then stops control, dataplane and bus. The `Drop` path kills actors.
- No gateway lookup code anywhere. No XML crate in `Cargo.lock`. `httparse`, `url`, `libc`,
  `if-addrs` and `socket2` are already in the lock.

### 3.2 New crate `ts_portmapper` (package `geiserx_ts_portmapper`)
```
ts_portmapper/
  src/lib.rs        Client, Config, Knobs, ProbeResult, MappingEvent, Error, PortMapReport
  src/client.rs     state machine (probe, create_or_get, invalidate, epoch, single-flight)
  src/gateway.rs    likely_home_router_ip(); self-IP fallback via if-addrs
  src/gateway/darwin.rs   sysctl(CTL_NET, PF_ROUTE, 0, AF_INET, NET_RT_DUMP2) + rt_msghdr2 parse
  src/gateway/linux.rs    /proc/net/route parser (private gateway only, line cap)
  src/lan.rs        LanUdp / LanHttp: the only I/O in the crate; destination allow-list
  src/pmp.rs        NAT-PMP build/parse (pure)
  src/pcp.rs        PCP build/parse (pure, IPv4-mapped bytes, no Ipv6 types needed)
  src/upnp/{ssdp,desc,soap,select}.rs     feature "upnp" (phase 2)
  src/testing.rs    TestIgd harness (cfg(any(test, feature = "test-igd")))
  fuzz/             cargo-fuzz targets (same layout as ts_tka/fuzz)
```

Public API, a 1:1 port of Go:
```rust
pub struct Config {
    pub gateway: GatewayLookup,      // Arc<dyn Fn() -> Option<(Ipv4Addr /*gw*/, Ipv4Addr /*self*/)>>
    pub knobs: Knobs,                // disable_all / upnp / pmp / pcp: Arc<AtomicBool> each
    pub pxp_port: u16,               // 5351; test override
    pub upnp_port: u16,              // 1900; test override
    pub ssdp_multicast: bool,        // false in tests
    pub bind_self_ip: bool,          // true in prod (LAN-scoped source), false in tests
    pub clock: Clock,                // Arc<dyn Fn() -> Instant>; manual clock in tests
}
impl Client {
    pub fn new(cfg: Config) -> (Self, tokio::sync::watch::Receiver<Option<MappingEvent>>);
    pub async fn probe(&self) -> Result<ProbeResult, Error>;
    pub fn have_mapping(&self) -> bool;
    pub fn get_cached_mapping_or_start_creating_one(&self) -> Option<SocketAddrV4>;
    pub fn set_local_port(&self, port: u16);
    pub fn note_network_down(&self);
    pub fn renew_after(&self) -> Option<Instant>;   // Rust addition, for the driver's timer
    pub async fn close(&self);                     // release, bounded (1 s)
    pub fn release_blocking_best_effort(&self);    // Drop path: std UdpSocket, PMP/PCP only
}
pub enum Error { Disabled, NoServices, GatewayRange, GatewayIpv6, NoMapping(String), Io(io::Error) }
```

Implementation notes:
- State lives in `std::sync::Mutex<State>`, never held across an `.await`. Release takes the
  mapping out under the lock and sends after unlocking.
- Background creation is `tokio::spawn`, tracked by the client's `JoinSet` and gated by an
  `AtomicBool running_create` (Go's single-flight).
- The mapping is an `enum Mapping { Pmp(..), Pcp(..), Upnp(..) }`.
- Instants use `checked_add` and clamp. A router-supplied lifetime of up to `u32::MAX` must not
  panic.
- `Clock` exists because `tokio::time::pause()` auto-advances while real UDP I/O is pending and
  would fire the 250 ms timeouts too early. Lease and renewal logic is tested with a manual
  clock; I/O timeouts use real time.

### 3.3 Anti-leak / IPv4-only fit
- **IPv4 only.** The crate refuses IPv6 gateways (`Error::GatewayIpv6`). A PCP external address
  that isn't IPv4-mapped is rejected. The crate binds no `[::]` socket.
- **LAN only.** The gateway must be a **private** IPv4 (RFC 1918) on both platforms; macOS
  doesn't check this in Go (Linux does). The own IP must be private as well.
- `lan.rs` is the crate's only I/O. Its UDP sockets bind `self_ip:0` (production), so a vanished
  interface fails closed. `send_to` refuses any destination other than
  `{gw:pxp_port, gw:upnp_port, 239.255.255.250:upnp_port}`. Multicast is sent with
  `IP_MULTICAST_IF = self_ip` and TTL 1.
- **UPnP HTTP.** The TCP connect target is always `gw:<port from Location>` (Go's repoint, made
  mandatory). Bodies are capped (root description 256 KiB, SOAP 64 KiB, SSDP 1500 B). No TLS,
  no DNS, no proxy. These sockets never reach the internet: they're the same state Go creates
  and they don't touch the "single egress socket" property of the overlay path.
- **Advertising.** Only advertise an external address that passes the same test as
  `is_pingable_candidate` (`sock.rs:837-868`: not private, loopback, link-local, unspecified or
  multicast) **and** is not CGNAT (100.64.0.0/10). This is stricter than Go, which advertises
  whatever the router reports.
- **New CI check `checks/src/portmapper_lan_only.rs`.** Register it in `CHECK_FNS`
  (`checks/src/main.rs:35-46`). It scans `ts_portmapper/src` (excluding `testing.rs`) and fails
  on:
  - `UdpSocket::bind`, `TcpStream::connect`, `send_to(` or `connect(` outside `lan.rs`;
  - `Ipv6Addr::UNSPECIFIED` or `"[::]"` anywhere;
  - any `reqwest` / `hyper-rustls` / `native-tls` dependency in `ts_portmapper/Cargo.toml`.

  `ipv4_only_host_net` and `ipv4_only_forwarder` are unaffected: no changes to `ts_host_net` or
  `ts_forwarder`.
- **Test isolation.** Existing `ts_runtime` tests that spawn `DirectManager` would otherwise
  probe the real LAN gateway. Test configs set `disable_port_mapper = true`, the Rust equivalent
  of Go's `DisablePortMapper: true` and `TS_DISABLE_PORTMAPPER=1`. Add a check or test helper
  that asserts this.

### 3.4 Integration points (Rust file:line)
1. **`ts_magicsock/src/endpoint.rs:13-25`.** Add `SelfEndpointType::PortMapped`.
2. **`ts_magicsock/src/sock.rs`.**
   - Add a `portmapped: Mutex<Option<SocketAddr>>` field (near `:343`).
   - Add `pub fn set_portmapped(&self, Option<SocketAddrV4>)`, which applies the advertising
     filter from §3.3.
   - `self_endpoints()` (`:914`) pushes the portmapped endpoint **first** (Go order).
   - `seal_call_me_maybe` (`:1116`) then picks it up automatically.
   - `rebind()` (`:700`) does **not** clear it; the driver decides (§3.5).
3. **`ts_runtime/src/control_runner.rs:2157-2160`.** Map `PortMapped` to
   `EndpointType::PortMapped`.
4. **`ts_runtime/src/direct.rs`.**
   - `run_call_me_maybe` (`:1035-1041`): treat `PortMapped` like `Stun` when deciding whether
     there is a reflexive address.
   - `on_start` (`:1114-1268`): build the `ts_portmapper::Client` unless disabled, and spawn
     `run_portmapper` (§3.5).
   - `rebind_and_reprobe` / `re_stun` (`:422`, `:461`): call `set_local_port(local_port)` and
     kick the driver.
   - `apply_self_node_knobs` (`:1362-1383`): set `knobs.disable_upnp` from
     `node.disable_upnp()` and `knobs.disable_all` from `only_tcp_443()` (Go wiring).
   - New message `ReleasePortMappings`, which calls `client.close()`.
5. **`ts_runtime/src/direct.rs:152-154` and `:971-1002`.** `EndpointAdvertisement` gains
   `port_map: PortMapReport { upnp, pmp, pcp: Option<bool>, have_port_map: bool }`. The
   advertiser compares `(endpoints, port_map)` with the last value it published, which gives
   Go's `BasicallyEqual` de-duplication for these fields.
6. **`ts_control/src/tokio/client.rs`.**
   - `CarriedNetInfo` (`:42-73`): add `upnp`, `pmp`, `pcp` and `have_port_map`, applied on every
     request.
   - `set_endpoints` / `Command::SetEndpoints` (`:229`, `:901-922`) carry the `PortMapReport`.
   - Leave `net_info_working_udp` / `net_info_mapping_varies` as they are. They count only
     `Stun`; a portmapped endpoint must not count as working UDP. That matches Go: `WorkingUDP`
     comes from STUN.
7. **`ts_control/src/map_request_builder.rs:78-102`.** Add `upnp()`, `pmp()`, `pcp()` and
   `have_port_map()` setters.
8. **`ts_control/src/node.rs:816-833`.** Add
   `disable_upnp() -> has_node_attr("debug-disable-upnp")`.
9. **Config chain.**
   - `src/config.rs:241-273`: add `pub disable_port_mapper: bool` (default `false`, i.e. enabled,
     the Go default) and `pub port_map_gateway: Option<(Ipv4Addr, Ipv4Addr)>` (manual override,
     the analogue of `SetGatewayLookupFunc`).
   - Pass both through `ts_control/src/config.rs:375-399` and `ts_runtime/src/env.rs:130-148` /
     `:316-330`.
   - Env knobs, read on every attempt as in Go: `TS_DISABLE_PORTMAPPER` and `TS_DISABLE_UPNP`.
   - Debug-only: `TS_DEBUG_PORTMAPPER_ONLY=pmp|pcp|upnp`, the equivalent of `debug portmap
     type=`.
10. **`ts_runtime/src/lib.rs:1475`.** `graceful_shutdown`: **before** `shutdown.send_replace(true)`,
    run `timeout(1s, direct.ask(ReleasePortMappings))`. In `Drop` (`:1505`), call
    `release_blocking_best_effort()` (PMP/PCP: one datagram, no runtime needed).
11. **`examples/exit_node/main.rs:30-103`, `:160-181`, `:265`.**
    - Add `--no-portmapper` and `--portmap-gateway GW[,SELF]`.
    - Add a `--portmap-probe` mode: probe, map, print, release, exit. This is the Rust
      `tailscale debug portmap`.
    - The status line prints a `portmap=active-MC ext=a.b.c.d:p` summary in Go's
      `portMapSummary` format (`tailcfg.go:1172-1185`).
12. **Docs and ledger.**
    - `PORTING.md:121`: move `net/portmapper` from "no counterpart" to the mapping table (the
      `porting_ledger` check enforces this).
    - `PORTING.md:2005` and `:2037-2045`: `DisableUPnP` and the four NetInfo facets now have a
      target.
    - Update `docs/PARITY_ROADMAP.md` and `docs/MIGRATION_STATUS.md` (`tsr-x4yg`).

### 3.5 The driver (`ts_runtime/src/portmap.rs`, run by `DirectManager`)
```
loop select {
  shutdown            => client.close().await; break
  jittered 20-26 s    => if should_do_periodic_restun(..) { tick() }     // Go: periodic ReSTUN
  kick.notified()     => tick()          // on_start, first peers, rebind, re_stun, knob change
  sleep_until(renew_after) if mapping exists && peers exist => tick()    // see §6 deviation
  mapping_rx.changed()=> push endpoint; wake advertiser
}
tick():
  client.set_local_port(sock.local_addr().port())
  report = client.probe().await  (Err(Disabled|GatewayRange) => all Some(false), Go parity)
  ext = client.get_cached_mapping_or_start_creating_one()
  sock.set_portmapped(ext); shared PortMapReport = {report, have_port_map: client.have_mapping()}
```

Notes:
- Gateway and network-change detection is the gateway lookup run on every tick. The sysctl or
  file read costs microseconds, so this stands in for netmon's cached `GatewayAndSelfIP`. When
  OS netmon backends land, call `note_network_down` / `set_local_port` from `NetmonSupervisor`
  too.
- Logging, which must survive release builds that strip debug:
  - `info`: mapping obtained (type, external, lease), mapping lost (epoch, gateway change,
    expired), released, probe result changed, disabled by knob.
  - `warn`: router refused (`NOT_AUTHORIZED`, error codes), malformed reply (once per gateway),
    LAN send error such as `EHOSTUNREACH`.
  - Per-tick traffic: `trace`/`debug` only. Repeated identical lines are suppressed until state
    changes, standing in for `LinkChangeLogLimiter`.
- Metrics: register Go's `portmap_*` counter names in `ts_metrics` (optional; Go names, with
  `minus_` handling for negative UPnP codes).

### 3.6 Dependencies and runtime
- Runtime: tokio, already used everywhere; nothing else.
- Phase 1 adds **no new crates**: tokio, tracing, rand, thiserror, libc (apple/linux), if-addrs
  and socket2 are all in `Cargo.lock`.
- Phase 2 (UPnP):
  - HTTP: either `ts_http_util::http1::connect(stream)` over a guarded `TcpStream`
    (`ts_http_util/src/http1.rs:71`) or a ~150-line hand-rolled HTTP/1.1 client on `httparse`
    (already locked). Prefer hand-rolled, for exact control of limits, chunked encoding and
    `Connection: close`, and to avoid `ts_http_util` logging connection errors at `error`.
  - XML: **one new dependency.** Recommended: `roxmltree` (no dependencies, read-only DOM,
    entity-expansion limits), plus a size cap and refusing any `<!DOCTYPE`. The alternative is
    a hand-rolled tag scanner for the five element names we need. Needs a `supply-chain/` audit
    entry and a `deny.toml` licence check (MIT/Apache).
  - SOAP requests are formatted strings with XML-escaped values.
- Cargo feature `portmapper` on `ts_runtime` and the facade, **default on** (Go parity), with an
  `upnp` sub-feature (default on).

---

## 4. Phasing

| Phase | Scope | Done when |
|---|---|---|
| **0** | `gateway.rs` (darwin sysctl + linux proc + override), `pmp.rs` / `pcp.rs` codecs, `--portmap-probe` runner mode, TestIgd (PxP half) | On kmini, `--portmap-probe` prints `gw=192.168.x.1 self=… PMP=true PCP=true` and gets plus releases a PMP mapping (visible in the router's table) **when launched the same way the daemon is** (see Local Network risk, §6) |
| **1** | Client state machine (probe, create, renew, epoch, single-flight, invalidate, release), driver, magicsock `PortMapped`, control type 3, NetInfo PMP/PCP/HavePortMap, knobs (env, config, `only-tcp-443`), graceful and Drop release, CI check, ledger | **The kmini case works.** The admin console / NetInfo shows PMP and PCP; the café client (`MappingVariesByDestIP: true`) gets `tailscale ping` "via <public>:<mapped>" and `tailscale status` direct within ~1 min of starting; the mapping is renewed at about 1 h; stopping the runner removes the router entry |
| **2** | UPnP-IGD: SSDP probe and collection, root description, service selection incl. legacy, SOAP add/delete/status/externalIP, 402/725 permanent retry, `TS_DISABLE_UPNP` + `debug-disable-upnp`, NetInfo UPnP | All Go UPnP tests ported and passing; manual test against kmini's router with PMP/PCP disabled via `TS_DEBUG_PORTMAPPER_ONLY=upnp` |
| **3 (optional)** | Beyond Go: epoch check on **every** PMP/PCP reply (RFC 6886 §3.6), listening for PMP multicast announcements (224.0.0.1:5350, a Go TODO at `portmapper.go:314`), persisting the last external port to limit permanent-lease UPnP leaks | Each change is a separate, reviewable commit |

kmini needs only phases 0 and 1. With PMP available Go uses PMP (`portmapper.go:697`), and PCP is
the fallback.

---

## 5. Test plan

### 5.1 Every Go test, and what happens to it
| Go test (file) | What it checks | Rust |
|---|---|---|
| `TestCreateOrGetMapping` (portmapper_test.go:18) | Live router; two create calls (HIT_NETWORK) | **Adapt**: `#[ignore]` live test, `TS_HIT_NETWORK=1`; also checks the second call is reused and that close releases |
| `TestClientProbe` (:34) | Live; three probes 100 ms apart | **Adapt**: live/ignored; logs results |
| `TestClientProbeThenMap` (:49) | Live; probe then map, verbose | **Adapt**: live/ignored |
| `TestProbeIntegration` (:63) | Fake IGD with all three services; UPnP detected; exact counters {UPnPDisco 1, PMP 1, PCP 1, PCPDisco 1, PMPPublicAddr 1} | **Port.** Phase 1 asserts the PxP counters plus `pmp && pcp` (the Rust harness answers PMP; Go's doesn't, `igd_test.go:218`); phase 2 adds UPnP |
| `TestPCPIntegration` (:97) | PCP-only IGD: probe sees only PCP; create gives a valid external; mapping stored | **Port** |
| `TestGetUPnPErrorsMetric` (:132) | Metric names for codes 100, 0, -100 don't panic (`minus_`) | **Port** if metrics are ported, else N/A |
| `TestUpdateEvent` (:139) | Mapping event published after `GetCached...` | **Adapt**: the `watch` receiver gets `MappingEvent` exactly once |
| `TestParsePCPMapResponse` (pcp_test.go:17) | Golden reply gives 135.180.175.246:1234 and nonce = bytes 24..36 | **Port** verbatim (same bytes) |
| `FuzzParsePCPResponse` (fuzz_test.go:8) | No panic; seed corpus | **Port**: cargo-fuzz target plus a proptest with the same seeds |
| `FuzzParsePCPMapResponse` (:43) | Never both a mapping and an error, never neither | **Port** (same invariant and seeds) |
| `TestSelectBestService` (select_test.go:21): `single_device`, `first_device_disconnected`, `prefer_public_external_IP`, `all_private_external_IPs`, `nothing_connected`, `GetStatusInfo_errors`, `GetExternalIPAddress_bad_ip` | Service-selection order and fallbacks; skip disconnected devices; prefer a public external IP | **Port** all 7 (phase 2) |
| `TestParseUPnPDiscoResponse` (upnp_test.go:418): google, pfsense, sagemcom, huawei | SSDP header parsing (Location, Server, USN), incl. empty `SERVER:` | **Port** with the same fixtures |
| `TestGetUPnPClient` (:458): google→WANIPConn2/single, pfsense→WANIPConn1/single, mikrotik→WANIPConn1/none, huawei→legacy PPP/single, not_supported→nil | Root-description parse plus selection plus the log line | **Port**; assert the service kind and `method`; the log assertion is adapted to `tracing-test` |
| `TestGetUPnPPortMapping` (:536) | testRootDesc and mikrotik; lease request gets 725, retry permanent; ext 123.123.123.123; second run with prevPort gives the same external | **Port** |
| `TestGetUPnPPortMapping_LeaseDuration` (:631): only_permanent_leases (725), invalid_args (402) | Both trigger a permanent retry | **Port** |
| `TestGetUPnPPortMapping_MikroTikDoesNotQueryExternalIPAfterMapping` (:720) | `GetExternalIPAddress` called exactly once | **Port** |
| `TestGetUPnPPortMapping_NoValidServices` (:780) | No panic and no mapping when no supported service exists | **Port** |
| `TestGetUPnPPortMapping_Legacy` (:811) | `urn:dslforum-org` PPP mapping works (`NewUpTime`) | **Port** |
| `TestGetUPnPPortMappingNoResponses/ErrorContactingUPnP` (:854) | Unreachable Location gives no mapping | **Port** |
| `TestProcessUPnPResponses` (:886): single, multiple_with_same_location, multiple_with_different_location | De-duplication and order (IGD:2 first) | **Port** |
| `TestGetUPnPPortMapping_Invalid` (:965): 0.0.0.0, 127.0.0.1 | External IP rejected; **0** `AddPortMapping` calls | **Port** |
| `igd_test.go` (harness, no tests) | Fake IGD, counters, `newTestClient` (gw 127.0.0.1, self 1.2.3.4) | **Adapt** into `TestIgd` (§5.3), extended |
| netcheck `TestReportString` portmap_all / portmap_some (net/netcheck/netcheck_test.go:992-1009) | Summary `portmap=UMC` / `UC` | **Adapt**: `PortMapReport::summary()` plus `active-` prefix |
| netmon `TestLikelyHomeRouterIP`, `_Prefix`, `_NoMocks` (interfaces_test.go:28, :129, :198) | Self-IP fallback: skips down and IPv6-only interfaces; prefix must contain the gateway; private | **Adapt** against injected interface lists; `_NoMocks` as a smoke test |
| netmon `TestExtremelyLongProcNetRoute` (interfaces_linux_test.go:40) | Route-file line cap | **Port** to the Linux parser |
| netmon `TestLikelyHomeRouterIPSyscallExec`, `TestFetchRoutingTable` (interfaces_darwin_test.go:17, :106) | sysctl result matches `netstat`/exec | **Adapt** (macOS only): compare with `route -n get default` |
| netmon `TestGoogleCloudRunDefaultRouteInterface`, `TestAwsAppRunnerDefaultRouteInterface`, `TestRouteLinuxNetlink` | Default-route *interface*, not the gateway | **N/A** |
| magicsock tests with `DisablePortMapper: true` | Keep tests off the LAN | **Adapt**: runtime test configs disable it, and a test asserts no LAN traffic in the default test config |

### 5.2 Cases Go doesn't cover (add these)
Codecs (unit tests plus proptest):
1. PMP build: golden bytes for the public-address request `{0,0}` and for MAP
   (internal, suggested port, lifetime); delete uses lifetime 0.
2. PMP parse:
   - version ≠ 0 rejected; MAP reply ≠ 16 bytes and address reply ≠ 12 bytes rejected;
   - all-zero address gives no valid address;
   - each result code 1-5 is reported, not treated as success.
3. PCP:
   - ANNOUNCE golden; MAP golden with an IPv4-mapped client IP;
   - delete: lifetime 0, port 0, suggested address `::ffff:0.0.0.0`, original nonce (Errata 3621);
   - an external address that isn't IPv4-mapped is rejected (IPv4-only node).
4. A lifetime of 0, 1 or `u32::MAX` in a reply doesn't panic (`checked_add`). Lifetime 0
   (`goodUntil = now`) means no mapping; the client doesn't hot-loop renewals (create attempts
   are bounded by the tick).

State machine (manual clock plus TestIgd):

5. **Protocol-selection matrix.** For every combination of {PMP, PCP, UPnP seen} × {disable
   pmp/pcp/upnp}, assert which request types reach the IGD (counters). This covers
   `preferPCP`, UPnP-only when both PxP protocols are disabled, and the skip-to-UPnP path when a
   probe in the last 5 s saw nothing.
6. **Cache.** A call before `renew_after` sends nothing.
7. **Renewal timing.** Advance the manual clock past lifetime/2: the renewal sends `prevPort` =
   the current external port, and PCP reuses the nonce. The router assigning a different port on
   renewal updates the endpoint and fires one event.
8. **Expiry.** Past `goodUntil`, `have_mapping()` is false; the next call returns `None` and
   starts a create.
9. **Single-flight.** 50 concurrent `get_cached...` calls give exactly one MAP request.
10. **Probe caching.** A second probe within 10 min sends no PMP/PCP when they were seen; after
    invalidation it sends again.
11. **Epoch.**
    - PMP: probe epoch < mapping epoch drops the mapping without a release packet and clears the
      PMP cache.
    - PMP: an equal or higher epoch keeps it; epoch 0 is ignored.
    - The same three cases for PCP.
    - Router-reboot scenario: `igd.reboot()` resets the epoch and the mapping table; the next
      probe detects it and the mapping is recreated and re-advertised.
12. **Gateway or own-IP change.** The lookup returns a new gateway or own IP: the old mapping is
    released **to the old IGD** (a second TestIgd receives nothing), then a new mapping is made
    via the new IGD.
13. **Lookup failure.** Gives `GatewayRange`, invalidates and releases.
14. **IPv6 gateway.** Gives `GatewayIpv6`; no packets are sent.
15. **Non-private gateway (production lookup).** `None`; no packets are sent.
16. **`set_local_port`.** Same port is a no-op. A new port releases the old mapping and the new
    MAP carries the new internal port.
17. **`note_network_down`.** Invalidates without release (no delete packet).
18. **`close()`.**
    - PMP: delete packet with the right internal and external port; PCP: delete with lifetime 0
      and the nonce.
    - Idempotent; calls after close do nothing.
    - Finishes within 1 s even when the IGD is gone.
    - `release_blocking_best_effort` works with no tokio runtime.
19. **Router errors.**
    - PMP MAP replies with codes 2, 3, 4 or 5: no mapping, a `warn` log, and the next tick
      retries without a hot loop.
    - PCP ANNOUNCE with `NOT_AUTHORIZED` or `ADDRESS_MISMATCH`: `pcp=false`.
    - PCP MAP with a non-OK code: no mapping.
20. **Malformed or hostile replies.**
    - Truncated, wrong version, wrong opcode, unknown version (gives `NoMapping`), garbage.
    - Oversize (> 1500 B) datagrams: dropped.
    - PCP with the wrong nonce: ignored and the loop continues; a later correct reply is
      accepted.
    - A reply from a non-gateway IP or the wrong source port: ignored.
    - A burst of 1000 junk packets doesn't extend the 250 ms deadline.
21. **No responder.** The probe returns all false within ≤ 300 ms; create gives
    `NoMapping(NoServices)`.
22. **Advertising filter.** The router reports a private, CGNAT, loopback or 0.0.0.0 external IP,
    or external port 0: nothing is advertised and `have_port_map` stays false (deliberate
    deviation, §6).
23. **Disabled.**
    - `TS_DISABLE_PORTMAPPER=1` (via an injected env reader): `probe` gives `Disabled`, the
      NetInfo facets are `Some(false)`, and zero packets are sent.
    - `TS_DISABLE_UPNP`: no SSDP.
    - Control `debug-disable-upnp` toggled live; `only-tcp-443` gives disable-all live.
    - Config `disable_port_mapper` creates no client and opens no socket.
24. **LAN guard.** `LanUdp::send_to` refuses anything other than gateway, UPnP port or SSDP
    multicast. `LanHttp` refuses a host that isn't the gateway. Location `http://evil:80/x` is
    repointed to the gateway.
25. **Logging.** Using `tracing-test`, assert the obtained, lost and released lines are emitted at
    `INFO`, and that 100 identical failing ticks produce one `warn`.

Integration in the Rust tree:

26. `self_endpoints()` lists `PortMapped` first. `seal_call_me_maybe` includes it.
    `run_call_me_maybe` runs with only a portmapped endpoint and no STUN one.
27. `control_runner` maps `PortMapped` to wire `3` (JSON golden). `CarriedNetInfo` carries
    `UPnP`, `PMP`, `PCP` and `HavePortMap` on **every** request, including a reconnect (extend
    the existing test at `ts_control/src/tokio/client.rs:1430-1480`). `WorkingUDP` doesn't flip
    to true from a portmapped endpoint alone.
28. Advertiser: a change in `PortMapReport` alone republishes; an unchanged one doesn't.
29. `rebind_and_reprobe` with a port change triggers `set_local_port`, release and a new mapping.
30. Runtime end-to-end: spawn the runtime with a TestIgd plus `port_map_gateway` override (TestIgd
    on 127.0.0.1): the mapping is obtained, the endpoint advertised, and `graceful_shutdown` makes
    the IGD see the delete.
31. Default runtime test config: no packet leaves for any gateway. Assert with an injected lookup
    that panics if it is ever called.

Gateway lookup:

32. macOS: parse captured route-dump fixtures. Cases:
    - several default routes, the `IFSCOPE` one skipped;
    - utun default route with no `RTF_GATEWAY` → `None`;
    - truncated buffer → `Err`, never a panic.
33. Linux: `/proc/net/route` fixtures. Cases:
    - public gateway skipped;
    - a down route skipped;
    - hex byte order;
    - the line cap.
34. Fuzz targets: `parse_pmp`, `parse_pcp`, `parse_pcp_map`, `parse_ssdp`, the root-description
    parser, `parse_soap_fault`, `parse_rt_dump`, `parse_proc_net_route`.

UPnP extras (phase 2):

35. External port never < 1024 or 0. Assert `NewExternalPort ≥ 1024` across 1000 random draws
    (TS-2023-006 regression).
36. `AddAnyPortMapping` on WANIPConnection2 and the returned port is honoured.
37. `DeletePortMapping` on release uses the right external port and `"UDP"`.
38. SOAP faults: 402 and 725 retry permanent; 718 (conflict) and other codes fail with no retry;
    a fault that isn't a `UPnPError` gives no code.
39. Root description:
    - `Transfer-Encoding: chunked` works;
    - a body over the cap is rejected;
    - `<!DOCTYPE …>`/entity bomb is rejected;
    - a Location with a hostname instead of an IP is an error;
    - an `https` Location is refused.
40. SSDP:
    - reply from a port other than 1900 containing `:InternetGatewayDevice:` is accepted;
    - a reply without it is ignored;
    - more than 10 replies are capped;
    - duplicates are removed.
41. A permanent lease is still treated as good for 2 h and re-checked.

### 5.3 The fake router harness (`ts_portmapper/src/testing.rs`)
- `TestIgd::new(opts) -> TestIgd` binds:
  - `pxp` UDP on 127.0.0.1:0;
  - `ssdp` UDP on 127.0.0.1:0;
  - `http` TCP on 127.0.0.1:0, a minimal HTTP/1.1 server on `httparse` (dev-dependency).
- `test_client(&igd)` builds a `Config` with:
  - gateway lookup → `(127.0.0.1, 1.2.3.4)`, as Go does;
  - the IGD's ports;
  - `ssdp_multicast: false`, `bind_self_ip: false`;
  - a manual clock.
- Behaviour, mutable at runtime (`igd.set(|b| …)`):
  - `pmp`, `pcp`, `upnp` enable flags;
  - `public_ip` (default 203.0.113.7), `epoch` (fixed or auto-advancing);
  - per-opcode `result_code` overrides, `lifetime` override;
  - `assign_port: fn(requested) -> u16` (default: requested, else 4242);
  - `drop_next: usize`, `delay: Duration`;
  - `reply_from_wrong_port`, `corrupt: Option<Truncate|BadVersion|WrongNonce|Garbage|Oversize>`;
  - `enforce_pcp_client_ip` (gives `ADDRESS_MISMATCH`, code 12);
  - UPnP: `desc: String` and `control: HashMap<path, HashMap<action, Handler>>`, where `Handler`
    is either a static body or `Fn(&[u8]) -> (u16, String)` (Go's `upnpServer`);
    `chunked: bool`; `ssdp_reply: Option<String>`.
- Observation:
  - `counters()` with Go's field names (`num_upnp_disco_recv`, `num_pmp_recv`,
    `num_pmp_public_addr_recv`, `num_pcp_recv`, `num_pcp_disco_recv`, `num_pcp_map_recv`,
    `num_pcp_other_recv`, `num_pmp_bogus_recv`, `num_failed_writes`, `invalid_pcp_map_pkt`) plus
    `num_pmp_map_recv` and `num_deletes`;
  - `requests()`: every decoded request with its source;
  - `mappings()`: the live table, including PCP nonces;
  - `reboot()`: epoch back to 0, table cleared.
- Unlike Go's harness, PMP is fully implemented: address and MAP replies, deletes, the mapping
  table.
- Lifecycle: tasks live in a `JoinSet` and are aborted on `Drop`; there is no logging after
  close, like Go's `closed` flag.
- Exposed to `ts_runtime` tests through the `test-igd` feature.

---

## 6. Risks and open questions
1. **macOS Local Network privacy.** On Sequoia and later, a non-root process may need Local
   Network permission to send to `192.168.x.1:5351` or to multicast. Otherwise sends fail with
   `EHOSTUNREACH`/`EPERM`. Go `tailscale netcheck` worked from Kasper's terminal, but kmini may be
   launched differently (`runner/start.sh`, launchd?). Run the phase-0 `--portmap-probe` in the
   **same launch context** before phase 1. Treat this error as "no services" and log one `warn`.
2. **Gateway discovery on macOS** means hand-rolled `sysctl` route-dump parsing with `libc`
   (unsafe, layout-sensitive). Mitigations: fixtures, fuzzing, a cross-check against
   `route -n get default`, and the `--portmap-gateway` override.
3. **Default on (Go parity) changes the fork's on-the-wire behaviour.** Every ~23 s while active
   it sends PMP/PCP/SSDP to the gateway where nothing answered (Go does the same). On a cloud VPS
   the gateway (for example AWS 172.31.x.1) is private, so probes go to the VPC router. That's
   harmless, but the fork's upstream may prefer default off. Decide: on for ts-exit; for the
   GeiserX upstream, ask.
4. **Deliberate deviations from Go, each needing explicit approval:**
   - (a) Don't advertise private or CGNAT external addresses. In a double-NAT setup PMP returns
     the inner router's address, so this costs nothing real.
   - (b) **Renew before expiry even when idle**, as long as a mapping and peers exist. Go renews
     lazily, so an idle kmini would let its mapping lapse after about 2 h. That stays recoverable
     via DERP, CallMeMaybe and ReSTUN, but costs ~30-60 s on the first café connect. The cost is
     one packet per hour.
   - (c) Bind LAN sockets to the machine's own IP (avoids PCP `ADDRESS_MISMATCH` on hosts with
     several IPs).
   - (d) Mandatory destination allow-list and HTTP host pinning.
   - (e, optional phase 3) Epoch check on every reply.
5. **Router restart detection** takes up to 10 min in Go (probes are skipped while the service
   counts as recently seen). Accept that, or adopt 4e.
6. **Release on crash or kill.** Only the graceful and Drop paths release. PMP/PCP leases expire
   after 2 h at most; **permanent UPnP leases (MikroTik) leak one rule per unclean restart**, the
   same as Go. Possible fix: persist the last external port in `--state-dir` (phase 3).
7. **XML dependency** (`roxmltree` vs hand-rolled) needs `cargo vet`/`deny` sign-off; parsing
   untrusted XML from the LAN is attack surface. Mitigations: size caps, no DTD, fuzzing.
8. **No OS netmon.** Network changes are caught only by the per-tick gateway check and manual
   `Device::rebind`. Wi-Fi changes on a laptop deployment could leave a stale endpoint for up to
   one tick (~23 s, or longer when idle). Hook `NetmonSupervisor` once the backends exist.
9. **Privacy for proxy-egress deployments.** A portmapped endpoint shows the host's public IP to
   tailnet peers, as STUN endpoints already do. Nothing new reaches the internet. Should
   `exit_proxy` deployments turn the portmapper off by default?
10. **Logging volume.** Go's `LinkChangeLogLimiter` depends on netmon. The Rust "log on state
    change" replacement must be tested (case 25) so release logs stay readable.
11. **Endpoint churn.** A router that hands out a new external port on every renewal makes the
    endpoint flap every hour. That's acceptable (Go has the same behaviour), but watch the
    `SetEndpoints` rate.
