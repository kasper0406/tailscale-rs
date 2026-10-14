# Brief 27 — recovery after peer moves; listener queue states

2026-10-14. **Two reproduced defects fixed.** The path selector can ignore fresh
pongs forever after an old fast path expires. The listener discards valid
peer-half-closed connections and can strand reset sockets with unread data.
These fit the symptoms; live attribution remains unconfirmed. No live process,
state directory, tailnet or remote was accessed. Laptop NAT data and the states
behind the live warnings were not supplied.

## Fixes and evidence

Worktrees are under `/Volumes/git/ts-exit/scratch/`. Production fix branches start
at `ts-exit/fixes` (`cfa5ff2`); neither fix changes connected sockets.

| Branch / worktree | Commit | Change |
|---|---|---|
| `fix/derp-stuck-27` / `derp-stuck-27` | `857d2ea` | Select only the endpoint that just ponged; expired best yields to it regardless of historical RTT. Add discovery debug logs. |
| `fix/listener-states-27` / `listener-states-27` | `c7298ec` | Accept `CloseWait`; explicitly reclaim discarded sockets; log their state. |
| `test/derp-perf-27` / `derp-perf-27` | `5542d98` | Combined candidate based on `ts-exit/perf` (`39973cd`), with both fixes (`9744424`, `cb57470`) and an Apple socket/batch regression. No live deployment. |

Commits have the requested trailers. Existing branch tips were left unchanged.

## Why recovery fails

References in this section are to `fix/derp-stuck-27`, except the connected-socket
references, which are to `test/derp-perf-27`.

- **Learning and reconciliation:** authenticated UDP Ping learns its source via
  `sock.rs:2672` → `add_peer_endpoints`; filtering is in `sock.rs:567`.
  Routable IPv4 sources pass; private, loopback, link-local, multicast, unspecified
  and disabled IPv6 fail. `path.rs:258` rejects new sources at 32 learned candidates.
  Neither limit was observed live. `direct.rs:1435` reconciles control endpoints through
  `path.rs:437`. Distinct learned sources survive unchanged netmaps and have no
  age-based pruning. An endpoint also advertised by control becomes `Netmap`;
  control removal can then prune it; a later authenticated ping can re-add it.
  Whole-peer removal clears state.
- **Probing:** `path.rs:553` selects a never-pinged source even during quiet
  discovery. After trust expires it selects all candidates subject to the 5s
  floor; the 2s runtime tick (`direct.rs:677`) therefore retries every 6s. The
  5ms quiet-down and 60s upgrade gate cannot suppress an untrusted peer. In-flight
  probes expire after 5s when a new probe is recorded; candidates/RTTs do not.
  Brief 25's send-error fix is already present and continues the sweep.
- **The wedge:** old `recompute_best` globally scanned *every historical RTT*.
  An old 2ms endpoint beat a freshly answering 60ms source indefinitely. Our
  existing guard refused to refresh best from a rival's pong, leaving the stored
  old best untrusted: `best_addr` returned `None`, hence DERP. Probing continued;
  successful pongs could not restore direct. An unrelated historical rival could
  also be promoted without answering. No restart or key rotation is required.
- **Fix:** `path.rs:734` compares the just-confirmed endpoint against the current
  best. While best is trusted, Go's `betterAddr` scoring/hysteresis remains.
  Once untrusted, the next matching pong wins; only a pong confirming best renews
  its 6.5s trust. Go does this in local `repos/tailscale-go` (`3bb8a906`),
  `wgengine/magicsock/endpoint.go:2004` (`betterAddr || bestUntrusted`). An inbound
  ping alone still does not grant outbound trust.
- **Socket routing:** our perf layer's `connected.rs:120` keys sockets by the
  exact destination address **and port**, never by peer or the old best. A new
  source uses the shared socket until its own 1MiB threshold; its connected
  socket binds the same local port. `sock.rs:539` sends disco to the requested
  address; `sock.rs:1560` receives through both shared and connected pumps.
  `sock.rs:1480` batches WireGuard to the current trusted best; disco sends are
  individual. Keeping an old connected socket alive does not override selection.
  The Apple test verifies source address/port, new best, batched data and inbound
  replies with flags on/off, using the existing loopback candidacy seam. The
  portable virtual NAT test covers learned-source persistence and an IP/port move.

**Upstream comparison uses cached refs, without fetching:** GeiserX `origin/main`
`650e0e2` has the same historical scan (`path.rs:706`). The network-move regression
fails there too, returning the dead old address rather than the new source: that
version re-trusts stale best, whereas our guard turns it into persistent DERP.
Official tailscale-rs `official/main` `b44db0d` has no magicsock/direct implementation
and does not share this selector defect. Newer remote tips were not checked.

## Listener states

References: `fix/listener-states-27`,
`ts_netstack_smoltcp_core/src/socket_impl/tcp/listener.rs:120`.

| State in queued socket | Reachability / correct handling |
|---|---|
| `SynReceived` | Handshake incomplete; keep half-open. |
| `Established` | Accept normally. |
| `CloseWait` | ACK+FIN can complete a handshake directly into this state; data+FIN can arrive before accept. Accept, preserve RX/EOF and allow the application to reply. Fixed in both queues and the current-socket pump. |
| `Listen` | SYN-stage RST returns to Listen. Existing `prune_reset_half_open` removes it immediately so it cannot capture another client's SYN. Defensive queue fallback aborts/reclaims. |
| `Closed` | RST after establishment or a timeout/IP invalidation; normal, not an invariant warning. Log state at debug and queue reclamation even with unread RX. |
| `FinWait1`, `FinWait2`, `Closing`, `TimeWait`, `LastAck` | Require a **local** close; peer FIN alone cannot create them. No application owns an unaccepted socket to close it. Treat as an invariant failure, warn with state, abort and reclaim rather than leave an ownerless socket. |
| `SynSent` | Requires an active `connect`; impossible for normal passive accept. Warn with state, abort and reclaim. |

Vendored smoltcp `socket/tcp.rs:1885` permits `SynReceived` + FIN → `CloseWait`;
`:1074` shows which transitions require local close. Recovery/pacing affects timing,
not these state transitions. `Closed` is therefore a plausible source of the live
underscore-arm bursts; the omitted state prevents proving it from those warnings.
The old underscore arm only dequeued. Empty Closed sockets could later be orphan
reaped, but Closed with unread RX was excluded by the orphan predicate
(`stream.rs:224`) and leaked buffers indefinitely. Closing states were also excluded.
The separate CloseWait arm actively closed and discarded a valid connection,
including its request data; that is a distinct possible cause of failed requests.
The specific TLS failures remain unattributed.

GeiserX `origin/main` shares the listener defects. Official tailscale-rs shares the
CloseWait discard and bare underscore dequeue in
`crates/net/overlay/ts_netstack_smoltcp/src/netcore/socket_impl/tcp/listener.rs:97`;
its broader cleanup differs, so the exact leak lifetime is not asserted there.

## Verification

Before-fix failures are saved in `scratch/derp-stuck-27-before-{move,rival}.log`,
`scratch/listener-states-27-before-{fin,reset}.log`,
`scratch/derp-perf-27-before.log`, and `scratch/derp-origin-27-before.log`.
The origin check compiled its unmodified path module plus the regression in a
standalone harness. The socket regression fails with flags **off**; both modes
pass after the fix.

- Virtual NAT: old fast mapping dies; control advertises an unreachable mapping;
  only the authenticated ping source answers. Reconcile and probe 151 rounds over
  15 virtual minutes. Also reject resurrection of an unrelated historical rival.
- Real TCP segments: handshake ACK+request+FIN and FIN in the accept queue retain
  request, EOF and reply capability. RST with unread RX is reclaimed from either
  queue. Every local closing state and accidental SynSent is reclaimed; existing
  reset-to-Listen regression still passes.
- Full `cargo test --locked --all-features`: fix magicsock **188** unit tests;
  fix core **28** + 3 integration + 3 doctests; combined perf **203** magicsock,
  **56** core + 3 integration + 3 doctests. All pass.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`,
  `cargo fmt --all -- --check`, and `git diff --check` pass in all three worktrees.
  Cargo was capped at four jobs. Commands used `nice -n 10`, but the sandbox denied
  `setpriority`; builds continued without the requested priority change. Stable
  rustfmt also reports the repository's existing nightly-option notices.

## Diagnostic exitnode build

`RUST_LOG` cannot undo `release_max_level_info`: the exitnode profile inherits
release, and the example's tracing feature statically removes debug calls. In a
**disposable worktree**, replace that feature with `release_max_level_debug`.
Adding debug alongside info is insufficient because the lower cap wins.

```sh
git -C /Volumes/git/ts-exit/repos/geiserx-tailscale-rs worktree add --detach \
  /Volumes/git/ts-exit/scratch/derp-diagnostic-27 test/derp-perf-27
cd /Volumes/git/ts-exit/scratch/derp-diagnostic-27
python3 - <<'PYBUILD'
from pathlib import Path
p = Path("Cargo.toml")
s = p.read_text()
assert s.count("release_max_level_info") == 1
p.write_text(s.replace("release_max_level_info", "release_max_level_debug"))
PYBUILD
CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/derp-debug-27-target \
  nice -n 10 cargo +1.95.0 build -j 4 --profile exitnode --locked \
  --features ring-aead --example exit_node
```

For Claude's separately authorized diagnostic run, use:

```sh
export RUST_LOG='info,ts_magicsock=debug,ts_runtime::direct=debug,ts_netstack_smoltcp_core::socket_impl::tcp::listener=debug'
```

Look for these debug lines, correlate peer/address and transaction ID locally,
and redact endpoints before sharing logs:

- `magicsock: learned peer candidates` — does the ping source appear in `accepted`?
  Check `dropping non-pingable peer candidate endpoint` and
  `dropping learned candidate: peer at MAX_LEARNED_CANDIDATES_PER_PEER` if absent.
- `magicsock: disco pong sent` — we answered the laptop's ping at `from`; proves
  only the laptop's direction, not our trusted path.
- `magicsock: disco ping sent` / `sending disco ping; continuing discovery` —
  probe destination and transaction ID, or an individual send error.
- `magicsock: disco pong received` — `solicited=true`, matching ID and measured RTT.
- `magicsock: pong path selection` — `previous`, `confirmed`, `best_untrusted`,
  `best`; after expiry the fresh confirmed source should be best.
- `magicsock: connected socket opened` / `magicsock: connected socket failed; staying on shared socket`.
- `half-open socket closed; reclaiming` / `accept socket closed; reclaiming`,
  or `half-open socket in unexpected state; aborting and reclaiming` /
  `accept socket in unexpected state; aborting and reclaiming` — includes state
  and handle outside the trace-only spans.

For per-packet socket fallback/receive errors and runtime trace errors, instead
replace the cap with `release_max_level_trace` and select `ts_magicsock=trace,
ts_runtime::direct=trace`. The debug build intentionally does not retain trace.
Verified: the optimized ARM64 exitnode build succeeded; the tracing feature tree
contains only `release_max_level_debug`, and the binary retains the new debug
messages. Binary: `/Volumes/git/ts-exit/scratch/derp-debug-27-target/exitnode/examples/exit_node`.
The candidate manifest was restored. No diagnostic binary was started.
