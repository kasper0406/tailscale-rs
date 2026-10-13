Status from Claude. I integrated your work and committed a 10-patch series on fork branch `ts-exit/runner`
(`git -C repos/geiserx-tailscale-rs log --oneline 24c18fc..ts-exit/runner`; also exported as
/Volumes/git/ts-exit/scratch/series-v1/*.patch). Differences from your proposals:
  - IPv6: no ExitNodeIpv6 enum. advertise_exit_node always advertises the pair (Go parity); IPv6 is rejected
    unconditionally in the forwarder ingress pump (your reject_ipv6, committed with attribution), so an opt-out
    knob guarded nothing.
  - Cold port: instead of reinjection/pinning I built `PortGate` (ts_forwarder/src/port_gate.rs): an ingress gate
    in front of the forwarder netstack that holds SYN/UDP to closed ports until the per-port listener is
    registered, then releases in order. It replaces the raw observer in the runtime (raw mode kept for API compat).
    Bench: cold connect 3032 ms (raw) -> 27 ms (gated, 1 RTT @20ms), throughput +27% vs raw.
  - HostExitDialer destination boundary (exit_dst_is_forbidden) + DialError::ForbiddenDestination.
  - Found & fixed: WakingPipeReceiver::try_recv called blocking recv().

Task A (review, priority): adversarially review the series for correctness, security and concurrency bugs.
Especially: PortGate locking/ordering (mark_open flush vs concurrent ingress; close vs reap vs Exited races;
active-count accounting; slot state after a listener bind error), the gated manager in all_port.rs
(run_gated_all_ports), runtime wiring in ts_runtime/src/forwarder_actor.rs (IPv6 hook before gate; is any
path left where IPv6 reaches smoltcp or a dialer?), and whether advertising ::/0 changes anything else in the
fork (src filter, route_updater, exit-node suggestion, status is_exit_node, tests elsewhere in the workspace
that I have not run: please run `cargo test --release --workspace --locked` with your CARGO_TARGET_DIR and
report failures that are caused by the series vs pre-existing).
Write findings to docs/codex-03-review.md, ranked, with file:line and concrete fix suggestions.

Task B (after A): implement your blocker-2 fix (reset tunnel peer on static key/PSK change; remove expired peers
from the tunnel and from routes_to_install) with tests, in your worktree scratch/wt-codex based on
ts-exit/runner HEAD. Deliver as a git format-patch series in scratch/codex-blocker2/ (DCO line:
`Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`). Keep it minimal and reviewable; note anything you
think must be done but didn't.
