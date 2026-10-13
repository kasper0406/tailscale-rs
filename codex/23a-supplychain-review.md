# Review 23a: malicious code and supply-chain review of the GeiserX fork (read-only)

Kasper runs an exit node built from a third-party fork (GeiserX/tailscale-rs, one maintainer, not affiliated with
Tailscale, heavy automation). He wants assurance there is nothing malicious or obviously dangerous in it. **Do not
modify any code or branches; write only your report.**

Repo: `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs` (remotes `origin` = GeiserX, `official` =
tailscale/tailscale-rs). GeiserX's history has no common commits with official: its root import `580057a` equals
official `29d87ee` (2026-05-27) except a 2-line change in `ts_control_serde/src/debug.rs` and docs. **Scope = `git
diff 29d87ee origin/main`** (623 commits, ~149k added lines, 441 files). Our own work (`ts-exit/*` branches) is out
of scope here.

Focus, in priority order:
1. **Exfiltration / hidden endpoints.** Every hard-coded URL, hostname, IP, port; HTTP/TCP/UDP clients and where
   they connect; anything contacting non-Tailscale infrastructure (telemetry, analytics, update checks, "phone
   home", paste/webhook services, crash reporters). Check control/DERP/log/"debug" paths for added destinations.
2. **Secrets handling.** Auth keys, node/machine/disco private keys, WireGuard keys, TKA keys, SSH host keys,
   proxy credentials: are they ever logged, written with loose permissions, sent anywhere other than control as
   upstream does, or exposed via debug/FFI APIs?
3. **Build-time and supply chain.** All `build.rs`, proc-macro crates, and new dependencies. From the Cargo.lock
   diff, list the new crates (name, version, source; registry vs git; any `[patch]`/git sources), and flag
   typosquat-looking names, very new/low-download crates (use crates.io API if network allows, else note
   "unchecked"), crates with build scripts or native code. Check `deny.toml` changes and `supply-chain/`.
4. **CI/release automation.** `.github/workflows/*`, `.github/actions/*`, `scripts/*.sh`: secrets usage,
   self-hosted runners, release/publish flows (could a release ship different code than the source?), anything
   downloading and executing remote scripts.
5. **Code-level red flags.** Obfuscation (encoded blobs, `include_bytes!` of opaque data, base64 payloads),
   `unsafe` added outside obvious FFI/syscall needs, `std::process::Command`/shell execution, filesystem writes
   outside the configured state dir, environment-variable backdoors (hidden env vars that disable checks),
   time-bomb style conditionals.

Method: use grep/ripgrep across the diff to triage, then read every hit in context. Be concrete: file:line,
what it does, risk level (malicious / dangerous / questionable / fine). Distinguish "suspicious" from "confirmed".

Write `/Volumes/git/ts-exit/docs/review-23a-supplychain.md`: verdict first (any malicious code found? yes/no, with
confidence), then findings by severity, the new-dependency table, endpoints table, and what you could not check.
Keep it factual and concise.
