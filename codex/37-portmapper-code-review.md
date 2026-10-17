# Review 37: independent review of the port mapper implementation (read-only)

A Claude workflow implemented a NAT-PMP/PCP port mapper for the Rust userspace Tailscale node, following
`docs/PORTMAPPER-PLAN-v2.md` (itself revised after your review 36, `docs/review-36-portmapper-plan.md`). Branch
`feat/portmapper` in `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs` (48 commits on GeiserX `origin/main`; worktree
`/Volumes/git/ts-exit/scratch/portmapper`). Implementation notes: `docs/PORTMAPPER-IMPL.md`. Go reference:
`/Volumes/git/ts-exit/repos/tailscale-go/net/portmapper`. It will run on the live exit node `kmini` (home Mac behind
an Asus/MiniUPnPd router), unattended. **Read-only: write only your report. Do not contact any real router or LAN
address; use the fake router harness for any experiments.**

Review `git diff origin/main..feat/portmapper` critically, focusing on what could hurt a live, unattended node:
1. **Router safety:** can it leak rules, storm the router, delete rules it doesn't own, or act on a different
   network after a change? Check the generation/ledger/cleanup logic, retry spacing, expiry, and shutdown release.
2. **Node safety:** panics or busy loops reachable from network input or timing; blocking calls on async tasks
   (getifaddrs, sysctl); unbounded growth; interaction with rebind, only-tcp-443, and the watchdog/shutdown paths.
3. **LAN-only boundary and hostile inputs:** the GatewayCap design, IP_BOUND_IF / SO_BINDTODEVICE / SO_DONTROUTE
   use, reply validation, spoofing (the acknowledged NAT-PMP no-nonce limitation), parser bounds.
4. **Integration correctness:** endpoint dedup/priority and STUN evidence, NetInfo facets (`active-_MC`), the
   CallMeMaybe gate, the runner flags and defaults (on by default?), and whether existing behaviour changed for
   users who don't want port mapping.
5. **Do the reviews' fixes hold?** Spot-check the medium findings from the workflow's phase 0/1 reviews (listed in
   `docs/PORTMAPPER-IMPL.md`) and whether their regression tests would fail without the fix.
6. **Test quality:** are the acceptance-gate tests (plan v2 §8) meaningful; any test that can't fail; flakiness risk.

Write `/Volumes/git/ts-exit/docs/review-37-portmapper-code.md`: verdict first (safe to field-test on kmini? safe
to run unattended? confidence), findings by severity with file:line, trigger and fix, then what you couldn't check.
Concise.
