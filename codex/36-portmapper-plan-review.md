# Review 36: critique the port mapper plan before implementation (read-only)

A Claude planning agent wrote `/Volumes/git/ts-exit/docs/PORTMAPPER-PLAN.md`: a NAT-PMP/PCP/UPnP-IGD port mapper
for the Rust userspace Tailscale node (`/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`, branch `ts-exit/main`),
modelled on Go's `net/portmapper` (`/Volumes/git/ts-exit/repos/tailscale-go`). Motivation: the live node `kmini`
(home Mac, easy NAT, router supports UPnP/NAT-PMP/PCP) can't get direct paths to clients behind hard NATs
(`MappingVariesByDestIP: true`), which Go solves with a router port mapping. Kasper wants Go parity, Go's tests
ported, and good extra coverage. **Read-only: write only your report.**

Review critically, checking claims against both code bases:
1. **Correctness of the Go analysis:** protocol selection order (does Go really prefer NAT-PMP over PCP?), timeouts,
   lease/renewal behaviour, epoch handling, UPnP flow, release-on-shutdown, knobs (env, control `DisableUPnP`
   etc.), how magicsock/netcheck use it. Flag anything wrong or missing, with file:line.
2. **The Rust design:** crate layout, API, integration points (verify the cited Rust file:lines), the gateway
   discovery approach (macOS routing sysctl, Linux /proc), interaction with existing STUN/endpoint reporting,
   rebind and network-change handling (there's no OS netmon), concurrency and cancellation, and the anti-leak /
   IPv4-only rules (`AGENTS.md`, `checks/`). Is the proposed `checks/src/portmapper_lan_only.rs` sound?
3. **The five deliberate departures from Go** (plan section 6): for each, is it better, neutral or risky? Would
   upstream (GeiserX) accept it? Recommend keep/drop.
4. **Router safety:** what state does this create on a home router, worst cases (leaked mappings after crashes,
   UPnP permanent rules, mapping storms, wrong gateway), and whether the plan bounds them. Security: anything an
   attacker on the LAN could exploit (spoofed NAT-PMP/PCP/SSDP responses, malicious IGD XML, SSRF via the
   UPnP control URL, which must not leave the LAN).
5. **macOS deployment:** the plan flags the Local Network privacy permission. Determine as far as you can whether a
   launchd *system daemon* running as a user (UserName key, see `deploy/launchd/`) is subject to the Local
   Network prompt for UDP to the gateway on 5351 and SSDP multicast, and what the phase-0 probe should check.
6. **Test plan:** is every Go test accounted for correctly (look at Go's test files yourself)? Which of the 41
   added cases matter most, which are missing (e.g. NAT-PMP result codes, PCP nonce mismatch, mapping external
   port differing from internal, router returning a private external IP, simultaneous network change during
   renewal, clock jumps), and is the fake-router harness design adequate?
7. **Phasing and effort:** is phase 1 (NAT-PMP + PCP) really sufficient for kmini? Estimate the effort per phase.

Write `/Volumes/git/ts-exit/docs/review-36-portmapper-plan.md`: verdict first (ready to implement / needs changes),
then findings by severity with concrete fixes, then the answers. Concise.
