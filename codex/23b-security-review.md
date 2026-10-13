# Review 23b: security review of the GeiserX fork code paths our exit node runs (read-only)

Kasper runs `examples/exit_node` from our branch `ts-exit/main` (layered on GeiserX/tailscale-rs main) as an exit
node for his devices. GeiserX is a one-maintainer third-party fork of tailscale/tailscale-rs; its root import
`580057a` equals official `29d87ee` except trivial differences, so **GeiserX's own changes are `git diff 29d87ee
origin/main`** in `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs` (remotes: `origin` GeiserX, `official`
tailscale/tailscale-rs). **Do not modify code or branches; write only your report.**

Goal: is there anything in GeiserX's changes that weakens security for our deployment: an exit node on a home
Mac, userspace only, serving the owner's own devices, with run flags in `RUNBOOK.md` section 2 (workspace
`/Volumes/git/ts-exit`)? Focus on what is compiled into and reachable in our build (`ts-exit/main`, default
features + `ring-aead`), and note anything dangerous that is merely compiled in.

Check, with file:line evidence:
1. **Inbound authorization:** packet filter / ACL enforcement, source-IP validation (a peer can't spoof another
   peer's tailnet IP), handling of unsigned or locked peers (TKA), capability checks. Any path where inbound
   traffic reaches the forwarder, peerAPI, SSH, Taildrop, Serve/Funnel or the app netstack without the filter.
2. **Exit forwarding:** destination guards (no LAN/loopback/link-local/metadata-service reach), proxy egress
   fail-closed behavior, DNS forwarding (`ts_runtime` peerAPI DoH / exit DNS: who may query, open-resolver
   risk, Host-header checks), UDP relay bounds.
3. **Features that could expose the host:** SSH server (russh), Taildrop, Serve/Funnel, peerAPI endpoints, debug
   endpoints, FFI/Elixir/Python bindings. Are any enabled by default or by control-sent node attributes in our
   runner? Can a control-plane message alone turn on something that opens the host?
4. **Crypto / tunnel:** WireGuard implementation changes in `ts_tunnel` (handshake, replay window, nonce use,
   key erasure, cookie/mac2), disco message authentication, DERP client trust (TLS verification bypasses,
   `InsecureForTests`-style flags reachable in production), control-plane TLS/Noise.
5. **Robustness that matters for a remote, unattended Mac:** panics reachable from network input,
   unbounded memory growth from peer input, file writes outside the state dir.

Compare against the official code where useful (e.g. "official rejects X here; GeiserX accepts it").
Write `/Volumes/git/ts-exit/docs/review-23b-security.md`: verdict first (safe to keep running for this use? with
confidence), then findings by severity (critical/high/medium/low) with file:line and a concrete
exploit/failure scenario, then what was out of scope or unchecked. Concise and factual.
