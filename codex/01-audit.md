You are a senior Rust/networking reviewer collaborating with another agent (Claude) on the plan in
/Volumes/git/ts-exit/docs/PLAN.md (read it first). Machine: Mac mini M4, macOS 26.6.2. Repos are cloned in
/Volumes/git/ts-exit/repos/: tailscale-rs (official, b44db0da), geiserx-tailscale-rs (fork, 24c18fcf; NOTE: no shared
git history with official), tailscaled-rs (f95330a6), tailscale-go (3bb8a906, reference only).
Release builds of both Rust workspaces are running in parallel; do NOT run `cargo build` on the whole workspace yourself
(you may `cargo check -p <crate>` or run targeted `cargo test -p ts_forwarder` if useful; use CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target).

Task: Phase 1 code audit of the FORK's server-side exit-node path. Verify code, not docs. For each of the 8 seams in
PLAN.md Phase 1 (route advertisement, peer policy/packet filter, packet delivery to forwarder + reply routing, TCP,
UDP, DNS, IPv6, lifecycle) answer with file:line evidence:
  - what is implemented, what is missing, what is wrong/risky (security first: can an unauthorized peer or a forged
    source address get egress? Can an exit-node user reach the Mac's LAN / loopback / other tailnet nodes, and is
    that consistent with Go's behaviour where ACL `autogroup:internet` controls this?).
  - How is HostExitDialer selected vs DirectDialer/ProxyExitDialer? Is there any public API/config to enable residential
    exit egress, or must an embedder wire it? Is the forwarder in all-port mode for exit nodes?
  - The all-port mode drops the first SYN per new destination port and relies on client SYN retransmit (~1 s RTO).
    Quantify impact for a browser exit-node workload and propose a fix (e.g. pre-open listeners for common ports,
    or patch smoltcp dispatch).
  - Is direct UDP (magicsock) used, or DERP only? What UDP socket(s) does magicsock use on macOS (connected? batching?)
  - Compare with what the official tailscale-rs at b44db0da lacks (does it have magicsock / direct UDP at all?).
Also look at tailscaled-rs: does it already provide a runnable daemon with `--advertise-exit-node` that wires
HostExitDialer? Is it usable as our runner, or should we write a ~200-line purpose-built runner binary against the fork's
`tailscale` crate API? Recommend.

Write your findings to /Volumes/git/ts-exit/docs/codex-audit-01.md (concise, evidence-based, ranked list of blockers
for a first working IPv4 exit node with the official iPad client, then risks, then performance seams). Disagree with
the plan where you think it's wrong.
