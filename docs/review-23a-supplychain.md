**Verdict: No confirmed malicious code found. Confidence: moderate for this static, diff-focused review; this is not assurance that the fork or its releases are safe.** No added maintainer-controlled runtime collector, covert telemetry, credential-upload service, encoded executable payload, or time bomb was identified. Several confirmed dangerous behaviors and supply-chain weaknesses need attention, especially key-file permissions, direct exit-node access to metadata/local services, and Tailnet Lock disabling without verification.

Reviewed 2026-10-13. Scope: `git diff 29d87ee17e734c8d7c2dc5db60f4c67df566aa30 3951752dc1aea1a126fcb3d271eceb36b1cdb48e` (`origin/main`): 441 files, 148,715 additions, 5,298 deletions. The checked-out `ts-exit/main` and all `ts-exit/*` work were excluded. Files and line numbers below refer to that immutable fork commit. No source, branch, dependency, hook, or CI job was modified or executed.

## Findings by severity

### High — dangerous, confirmed behavior; malicious intent not established

1. **New state-store path can expose private identity keys locally.** [src/tsnet.rs:179](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/src/tsnet.rs#L179) uses `create_dir_all` and `std::fs::write` without restrictive modes, ownership checks, symlink refusal, or atomic replacement. [src/tsnet.rs:658](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/src/tsnet.rs#L658) serializes `PersistState`, containing machine, node/WireGuard identity, network-lock private keys and optional ACME account key. With ordinary Unix umask `022`, newly created directories/files normally become `0755`/`0644`; another local user may read the keys. Existing writable paths/symlinks also permit redirection or truncation. The new `Server::dir` path at [src/tsnet.rs:680](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/src/tsnet.rs#L680) reuses `src/config.rs:1096`, whose identical permission weakness is **inherited from the baseline**, not introduced by this diff. New `FileStore` is an added instance. Protect existing state directories/files externally; the implementation itself does not enforce protection.

2. **Direct host-exit forwarding lacks destination isolation.** [ts_forwarder/src/dialer.rs:143](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_forwarder/src/dialer.rs#L143) ignores `FlowClass` and opens host TCP/UDP sockets to the requested destination. [ts_runtime/src/forwarder_actor.rs:85](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_runtime/src/forwarder_actor.rs#L85) selects it when `forward_exit_egress=true` and no proxy is configured. `ts_forwarder/src/class.rs:41` classifies every IPv4 destination under an advertised `0.0.0.0/0`; `ts_forwarder/src/tcp.rs:102` uses the original overlay destination. Moreover [ts_dataplane/src/lib.rs:18](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_dataplane/src/lib.rs#L18) expressly permits `169.254.169.254` past the link-local prefilter. An exit-authorized peer whose ACL/port permissions allow the flow can therefore make the node connect to its cloud metadata service and potentially loopback/private services. **This is a static reachability finding, not a demonstrated exploit; host routes, ACLs, netstack handling and metadata authentication determine actual exposure.** The protective destination check exists only in `ProxyExitDialer` (`ts_forwarder/src/dialer.rs:277,314`), not `HostExitDialer`. Subnet forwarding intentionally permits destinations in operator-advertised internal routes.

3. **Control can clear Tailnet Lock enforcement without a verified disablement secret.** [ts_runtime/src/control_runner.rs:461](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_runtime/src/control_runner.rs#L461) discards the stored chain and sets the authority to `None` whenever the control-pushed status is not enabled; [ts_control/src/tka.rs:35](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_control/src/tka.rs#L35) defines that as a missing/empty head or `disabled=true`. No disablement-secret validation occurs on this branch, although `ts_control/src/tka.rs:21` explicitly says it should. [ts_runtime/src/peer_tracker/mod.rs:368](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_runtime/src/peer_tracker/mod.rs#L368) admits peers when authority is `None`; it also explicitly admits all on an empty trusted-key set. `ts_runtime/src/control_runner.rs:584` clears authority on a sync result reporting no genesis. A compromised or incorrect control plane can remove this independent peer-signature safeguard. The no-authority case also covers initial synchronization failures; ordinary packet ACLs still apply. **Confirmed fail-open logic**, relevant if relying on Tailnet Lock; no claim of a hidden backdoor or cryptographic break.

### Medium — dangerous or questionable, conditional

4. **Secret debug exposure, plus retained auth-key trace logging.** [ts_control/src/wif.rs:82](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_control/src/wif.rs#L82) derives `Debug` over auth keys, OAuth secrets and OIDC JWTs. [ts_keys/src/keystate.rs:21](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_keys/src/keystate.rs#L21) derives `Debug` over the new `acme_account_key: Option<Zeroizing<Vec<u8>>>`; `Zeroizing` zeroizes on drop but its `Debug` prints the inner bytes (`zeroize 1.9.0`, `src/lib.rs:602`). TKA request/response structs at [ts_control_serde/src/tka_mutation.rs:138](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_control_serde/src/tka_mutation.rs#L138), `ts_control_serde/src/tka_mutation.rs:85`, and `ts_control_serde/src/tka_bootstrap.rs:44` print raw disablement secrets. `ts_control_serde/src/id_token.rs:38` prints its JWT. **Confirmed debug/API exposure; no production call logging these complete values was found.** WIF errors also include up to 512 bytes of server response (`ts_control/src/wif.rs:313`), logged at `src/lib.rs:344`; ID-token errors log a similar preview (`ts_control/src/tokio/id_token.rs:200`). A server that echoes a submitted secret could put it in logs; that echo was not observed. Separately, `ts_control/src/tokio/register.rs:221,236` **actually logs the serialized registration body, including `Auth.AuthKey`, at trace level**. This log is inherited from baseline `register.rs:139`, not added malicious behavior, but remains active in release builds when trace logging is enabled and now also receives auth keys minted by the new WIF flow. `Device::new_with_secret` does not prevent that downstream plaintext logging.

5. **Proxy credentials are sent over plaintext host TCP.** [ts_forwarder/src/dialer.rs:238](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_forwarder/src/dialer.rs#L238) opens an ordinary TCP socket to the operator-configured proxy. SOCKS5 username/password bytes are written at `ts_forwarder/src/dialer.rs:448`; HTTP CONNECT sends base64 Basic credentials at [ts_forwarder/src/dialer.rs:479](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_forwarder/src/dialer.rs#L479). There is no proxy TLS transport. An observer between node and proxy can obtain credentials; the proxy necessarily sees destination addresses and tunneled traffic. This is documented protocol behavior, not an added hard-coded provider or covert exfiltration. Credentials are redacted from both proxy-config `Debug` implementations. Handshake failures drop the flow; UDP is refused rather than sent directly.

6. **Releases are not tightly bound to a verified source/dependency tree.** [scripts/publish-crates.sh:44](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/scripts/publish-crates.sh#L44) edits inter-crate version requirements in `Cargo.toml`, publishes with `--allow-dirty` and no `--locked` (`scripts/publish-crates.sh:136`), and may run `cargo update` during retries (`:172`). [.github/workflows/release-binaries.yml:68](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/.github/workflows/release-binaries.yml#L68) builds without `--locked`, restores compiler/build caches, and uploads with `gh release upload --clobber` (`.github/workflows/release-binaries.yml:103`). Its C-library release has no checksum manifest or attestation/signing step. `release.yml:38` has no CI `needs` dependency, environment gate, or tag-only check for manual dispatch; `release-please.yml:105` enables automatic merging with a configured PAT. **Published manifests are deliberately changed; dependency resolution/assets can differ from the reviewed tag. No malicious source substitution was found.** Branch protection, actual releases and runner/cache integrity were not verified.

7. **Dependency policy coverage is narrower than upstream, and build tooling is partly mutable.** [deny.toml:64](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/deny.toml#L64) disables all-features scanning and excludes development dependencies; the fork's real audit at [.github/workflows/ci.yml:164](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/.github/workflows/ci.yml#L164) runs without default features. The all-feature audit is owner-gated to `tailscale` (`ci.yml:335`). `supply-chain/` is byte-for-byte unchanged: empty local `audits.toml`, 324 exempted package names, old version exemptions/imports, no entries for the 35 new external names, and no `cargo vet` CI invocation. This is not audit evidence for this fork. The newly added provisioning action downloads and executes unpinned `https://sh.rustup.rs` (`.github/actions/provision-self-hosted/action.yml:58`); it is skipped on GitHub-hosted runners. `cargo-deny`/`cross` installs are unversioned (`ci.yml:157,316`), Cross images are not digest-pinned, and `actions/stale@v11` is tag-pinned. Most other actions use full commit SHAs; inherited `cargo-machete@main` is in a disabled upstream-only job. **Questionable supply-chain controls, not evidence of malicious tooling.**

8. **Optional trust/input overrides can redirect sensitive requests or weaken authentication.** [ts_control/src/tokio/connect.rs:199](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/ts_control/src/tokio/connect.rs#L199) allows plain-HTTP `/key` bootstrap with `allow_http_key_fetch=true`; a network attacker can substitute the control public key on that path. Default is HTTPS; the compile-time `insecure-keyfetch` escape hatch already existed upstream. OAuth secrets may specify an arbitrary `?baseURL=` (`ts_control/src/wif.rs:133,358`), which receives the client secret and API bearer token. Ambient GitHub OIDC trusts `ACTIONS_ID_TOKEN_REQUEST_URL` (`wif.rs:382`). These inputs require operator/environment trust; no malicious default destination was found. `TS_RS_EXTRA_CA_PEM` (`ts_tls_util/src/lib.rs:27,38`) adds roots to the **shared** TLS store, including ACME/WIF clients, not just control. It retains verification but expands trust. These are explicit configuration features, not hidden environment backdoors.

### Low — questionable development behavior

9. **The local ACME test helper exposes test services on all interfaces.** [scripts/pebble-up.sh:99](https://github.com/GeiserX/tailscale-rs/blob/3951752dc1aea1a126fcb3d271eceb36b1cdb48e/scripts/pebble-up.sh#L99) binds Pebble directory/management ports `14000`/`15000` to `0.0.0.0`; `pebble-up.sh:119` binds challenge DNS/management on `:8053`/`:8055`. It uses bundled test private keys and `curl -k` only against its test CA, and installs a version-pinned Let's Encrypt Pebble tool with `go install`. `pebble-down.sh:18` sources a generated PID file. These are explicit development commands, not daemon startup behavior; running them on a shared/public host exposes test control services and requires trusting their writable state directory.

## Other reviewed secret-handling and code paths

- Machine/node/disco/network-lock private-key types now redact `Debug` and zeroize on drop (`ts_keys/src/macros.rs:199,208`; network-lock type in `ts_keys/src/lib.rs`). `Display`, `to_bytes`, persistence serialization and existing FFI key-loading APIs intentionally remain capable of exporting raw keys. No new network upload of these private keys was found. Control registration carries auth credentials and public keys over Noise; new ID-token, DNS and TKA RPCs use the same configured control destination. TKA init/disable explicitly sends operator-supplied disablement secrets to control (`ts_control/src/tokio/tka_mutation.rs:139`), rather than to another service.
- LocalAPI/SOCKS credentials are randomly generated, listeners bind `127.0.0.1` on dynamic ports, and credentials are redacted (`src/loopback.rs:250,272`; `src/tsnet.rs:472,502`). New token/certificate/key APIs expose secrets to the in-process caller intentionally; status/WhoIs expose public identity data. SSH host private keys come from the caller's `russh::server::Config`, not a new hidden file or endpoint.
- Netmap/TKA cache files enforce Unix private ownership/modes, no-follow reads and exclusive writes (`ts_control/src/tokio/netmap_cache.rs:474,501,527`). **Cache correctness caveat:** the new cache stores full netmap frames without applying subsequent peer deltas (`ts_control/src/tokio/netmap_cache.rs:25`); a cold start can temporarily replay a removed peer until a fresh control map arrives. This is documented, conditional on configured/control-granted caching, and merits caution around revocation. Taildrop paths are explicitly configured and validate names/use no-follow opens (`ts_runtime/src/taildrop.rs:211`). Capture/Taildrop FFI, Python and Elixir methods can create files at caller-supplied paths, outside the state directory, by design. Capture records decrypted packet contents when explicitly installed (`src/lib.rs:1262`; `ts_ffi/src/capture.rs:39`); output files use ordinary permissions and may contain application secrets. No automatically enabled capture/export destination was found.
- Added process execution is host route/DNS configuration via argument arrays (`ts_host_net/src/linux.rs:115,136`; macOS `route`/`scutil`, `:139,159`) and the explicitly invoked SSH login-shell service (`src/ssh/shell.rs:488`). Linux helpers resolve `ip`/`resolvectl` via PATH. SSH clears inherited environment and attempts supplementary-group/gid/uid changes before exec. These are expected privileged capabilities, not hidden shell commands. No shell interpolation of network-provided command text was identified on those paths.
- Added production `unsafe` hits are C FFI pointer handling, host-information syscalls, cache ownership checks, and SSH pre-exec privilege changes. No unexplained executable blob, runtime `include_bytes!` payload, or obfuscated endpoint was found. Base64 uses are HTTP auth, DNS-over-HTTPS queries, ACME JWS/CSR, and TKA protocol/persistence. Large hex/JSON blobs are named crypto test vectors with generator sources. Binary additions are documentation imagery. Date conditions checked were expiry/retry/policy logic or test fixtures, not calendar-triggered payloads. Metrics are local counters/snapshots (`ts_metrics/src/lib.rs`); `auto_update_check` is a carried preference with no implemented remote updater. The `Debug.disable_log_tail` wire field is explicitly ignored; no new logtail uploader was found.

## New dependencies

The root lockfile has **35 genuinely new external crate names** and **127 added versions of previously present external names** (162 added external name/version/source tuples). All 643 external packages in the final root lockfile use `registry+https://github.com/rust-lang/crates.io-index`; no git sources, alternate registry, `[patch]` or `[replace]` was found in Cargo manifests/configuration. New workspace names use local paths, not registry artifacts during this source build.

For all 162 added external tuples, registry archives were downloaded read-only and SHA-256 checked against `Cargo.lock`: **162/162 matched**. That establishes correspondence with the lockfile, not author trust or absence of malicious code. New-name metadata was queried from the crates.io API on the review date. “Recent” means first published within 180 days; “low download” threshold was fewer than 100,000 downloads of the locked version. **None met that low-download threshold; no obvious typosquat name was identified.** Metadata repository links are self-reported, not independently verified identities. Counts are rounded, and downloads are not a security guarantee.

| New external crate | Locked version | Source | Locked-version downloads | Review note |
|---|---|---|---:|---|
| [approx](https://crates.io/api/v1/crates/approx) | 0.5.1 | crates.io registry | 133.60M | Colour-math test/support |
| [by_address](https://crates.io/api/v1/crates/by_address) | 1.2.1 | crates.io registry | 29.44M | Palette derive support |
| [critical-section](https://crates.io/api/v1/crates/critical-section) | 1.2.0 | crates.io registry | 94.32M | Ratatui/platform synchronization |
| [des](https://crates.io/api/v1/crates/des) | 0.9.0 | crates.io registry | 4.24M | SSH cipher; legacy algorithm support |
| [ed25519-zebra](https://crates.io/api/v1/crates/ed25519-zebra) | 4.2.0 | crates.io registry | 1.13M | TKA signature verification |
| [fast-srgb8](https://crates.io/api/v1/crates/fast-srgb8) | 1.0.0 | crates.io registry | 20.26M | Ratatui colour conversion |
| [ff](https://crates.io/api/v1/crates/ff) | 0.14.0 | crates.io registry | 7.15M | Finite-field arithmetic |
| [group](https://crates.io/api/v1/crates/group) | 0.14.0 | crates.io registry | 7.13M | Curve/group arithmetic |
| [if-addrs](https://crates.io/api/v1/crates/if-addrs) | 0.15.0 | crates.io registry | 5.31M | OS interface FFI; no build script |
| [jiff](https://crates.io/api/v1/crates/jiff) | 0.2.35 | crates.io registry | 24.55M | Date/time support via serde_with |
| [jiff-core](https://crates.io/api/v1/crates/jiff-core) | 0.1.0 | crates.io registry | 27.04M | Recent (2026-06-30); Jiff family |
| [jiff-static](https://crates.io/api/v1/crates/jiff-static) | 0.2.35 | crates.io registry | 15.49M | Proc macro; reads explicit TZif input files |
| [jiff-tzdb](https://crates.io/api/v1/crates/jiff-tzdb) | 0.1.8 | crates.io registry | 13.89M | Bundled timezone data |
| [jiff-tzdb-platform](https://crates.io/api/v1/crates/jiff-tzdb-platform) | 0.1.3 | crates.io registry | 49.04M | Platform timezone selection |
| [palette](https://crates.io/api/v1/crates/palette) | 0.7.6 | crates.io registry | 19.16M | Build script generates OUT_DIR/named.rs |
| [palette_derive](https://crates.io/api/v1/crates/palette_derive) | 0.7.6 | crates.io registry | 19.21M | Proc macro; colour trait code generation |
| [pem](https://crates.io/api/v1/crates/pem) | 3.0.6 | crates.io registry | 135.62M | ACME certificate encoding |
| [phc](https://crates.io/api/v1/crates/phc) | 0.6.1 | crates.io registry | 5.86M | Password-hash format support |
| [pkg-config](https://crates.io/api/v1/crates/pkg-config) | 0.3.34 | crates.io registry | 46.27M | Native-build helper for aws-lc-sys; runs system pkg-config |
| [portable-atomic-util](https://crates.io/api/v1/crates/portable-atomic-util) | 0.2.7 | crates.io registry | 35.84M | Build script probes rustc/target; no remote execution |
| [pty-process](https://crates.io/api/v1/crates/pty-process) | 0.5.3 | crates.io registry | 3.82M | Optional SSH PTY/process and OS syscalls |
| [rand_pcg](https://crates.io/api/v1/crates/rand_pcg) | 0.10.2 | crates.io registry | 63.14M | QUIC dependency; not identified as private-key RNG |
| [ratatui-termina](https://crates.io/api/v1/crates/ratatui-termina) | 0.1.0 | crates.io registry | 8.70M | Recent (2026-06-19); Ratatui terminal backend |
| [rcgen](https://crates.io/api/v1/crates/rcgen) | 0.13.2 | crates.io registry | 31.85M | ACME/TLS key and CSR generation; ring backend |
| [rustls-pemfile](https://crates.io/api/v1/crates/rustls-pemfile) | 2.2.0 | crates.io registry | 191.78M | Unmaintained advisory ignored; see below |
| [ruzstd](https://crates.io/api/v1/crates/ruzstd) | 0.8.3 | crates.io registry | 7.06M | Pure-Rust control-map decompression |
| [secrecy](https://crates.io/api/v1/crates/secrecy) | 0.10.3 | crates.io registry | 98.53M | Secret wrapper; explicit String conversion at Device boundary |
| [sponge-cursor](https://crates.io/api/v1/crates/sponge-cursor) | 0.1.0 | crates.io registry | 7.90M | Recent (2026-05-08); RustCrypto SHA-3 support |
| [ssh-key](https://crates.io/api/v1/crates/ssh-key) | 0.7.0-rc.11 | crates.io registry | 1.98M | Prerelease rc.11; RustCrypto SSH dependency |
| [termina](https://crates.io/api/v1/crates/termina) | 0.3.3 | crates.io registry | 8.74M | Terminal platform APIs/FFI; no native build script |
| [twox-hash](https://crates.io/api/v1/crates/twox-hash) | 2.1.2 | crates.io registry | 74.61M | Decompression support |
| [wnaf](https://crates.io/api/v1/crates/wnaf) | 0.14.0 | crates.io registry | 3.22M | Recent (2026-06-09); RustCrypto curve arithmetic |
| [wycheproof](https://crates.io/api/v1/crates/wycheproof) | 0.6.0 | crates.io registry | 397k | Development-only crypto vectors |
| [yasna](https://crates.io/api/v1/crates/yasna) | 0.5.2 | crates.io registry | 86.06M | ASN.1 for rcgen |
| [zlib-rs](https://crates.io/api/v1/crates/zlib-rs) | 0.6.7 | crates.io registry | 22.53M | Pure-Rust compression, not bundled C zlib |

New-name build scripts: only `palette 0.7.6` (`build/main.rs` → `build/named.rs:3`, local SVG colour table to `OUT_DIR`) and `portable-atomic-util 0.2.7` (`build.rs:127`, compiler/version probing). New-name proc macros: `palette_derive 0.7.6` and `jiff-static 0.2.35` (`src/lib.rs:90` reads the explicitly supplied timezone file). Their inspected behavior showed no outbound connection or remote-script execution. No C/C++/assembly source files were bundled in these 35 new-name archives; OS FFI and linkage remain relevant.

The sole repository `build.rs`, `ts_ffi/build.rs:5`, is **unchanged**: it runs cbindgen and writes `tailscale.h` to `OUT_DIR` and, unless `TS_FFI_BUILDRS_STRICT` is set, the crate source directory. No repository-defined proc-macro crate was found. Changed external build scripts mostly generate cfg/private modules, probe compiler capabilities, or locate Python/native libraries. `aws-lc-sys 0.45.0` is the substantial native C/assembly build; it invokes local build tools and supports prebuilt NASM objects. `pyo3`/`pyo3-ffi` use native Python configuration/linking; `tun-rs` has platform bindings; inherited `ring 0.17.14` compiles C/assembly and is unchanged in the lockfile. The default no-SSH exit path uses ring, while optional SSH and some development graphs include aws-lc. Full native-code/unsafe correctness review was not performed.

`deny.toml:18` adds an ignore for the new `rustls-pemfile` dependency's [unmaintained advisory](https://rustsec.org/advisories/RUSTSEC-2025-0134.html), not a reported exploitable vulnerability. Its comments about SSH advisories are stale: final `russh` is `0.63.3`, above the `0.60.3` fix for [RUSTSEC-2026-0153](https://rustsec.org/advisories/RUSTSEC-2026-0153.html) and [RUSTSEC-2026-0154](https://rustsec.org/advisories/RUSTSEC-2026-0154.html). Those specific advisories should not be reported as affecting this final tree. No complete current RustSec audit was run.

<details>
<summary>All 127 added versions of existing external crates (source: crates.io registry)</summary>

B = declared build script; P = proc macro; N = substantial bundled native source. Blank means none of these manifest/archive flags, not “safe”. `cbindgen` contains C/C++ test fixtures, rather than a native runtime build. Versions below were archive/checksum-checked and statically screened; per-version download/age checks were limited to the 35 new names above.

| Crate | Added locked version | Build/macro/native flag |
|---|---|---|
| aead | 0.6.1 | — |
| aes | 0.9.1 | — |
| aes-gcm | 0.11.0 | — |
| argon2 | 0.6.0 | — |
| aws-lc-rs | 1.18.1 | B |
| aws-lc-sys | 0.45.0 | B, N |
| base64 | 0.23.1 | — |
| bcrypt-pbkdf | 0.11.0 | — |
| bitflags | 2.13.0 | — |
| blake2 | 0.11.0 | — |
| blowfish | 0.10.0 | — |
| bytes | 1.12.1 | — |
| cbindgen | 0.29.4 | B, C/C++ fixtures |
| cfg-if | 1.0.5 | — |
| chrono | 0.4.45 | — |
| clap | 4.6.7 | — |
| clap_builder | 4.6.7 | — |
| clap_derive | 4.6.7 | P |
| cmov | 0.5.4 | — |
| crossbeam-epoch | 0.9.20 | B |
| crypto-bigint | 0.7.5 | — |
| curve25519-dalek | 5.0.0 | B |
| darling | 0.24.1 | — |
| darling_core | 0.24.1 | — |
| darling_macro | 0.24.1 | P |
| der | 0.7.10 | — |
| ecdsa | 0.17.0 | — |
| ed25519 | 2.2.3 | — |
| ed25519 | 3.0.0 | — |
| ed25519-dalek | 2.2.0 | — |
| ed25519-dalek | 3.0.0 | — |
| elliptic-curve | 0.14.1 | — |
| event-listener | 5.4.2 | — |
| flate2 | 1.1.10 | — |
| futures | 0.3.34 | — |
| futures-channel | 0.3.34 | — |
| futures-core | 0.3.34 | — |
| futures-executor | 0.3.34 | — |
| futures-io | 0.3.34 | — |
| futures-macro | 0.3.34 | P |
| futures-sink | 0.3.34 | — |
| futures-task | 0.3.34 | — |
| futures-util | 0.3.34 | — |
| getrandom | 0.4.3 | B |
| h2 | 0.4.19 | — |
| http | 1.5.0 | — |
| http-body-util | 0.1.5 | — |
| hybrid-array | 0.4.14 | — |
| hyper | 1.11.1 | — |
| hyper-util | 0.1.21 | — |
| ipnet | 2.12.2 | — |
| lazy_static | 1.5.1 | — |
| libc | 0.2.190 | B |
| lru | 0.18.0 | — |
| md5 | 0.8.0 | — |
| miniz_oxide | 0.9.1 | — |
| ml-kem | 0.3.2 | — |
| num-bigint | 0.5.1 | — |
| p256 | 0.14.0 | — |
| p384 | 0.14.0 | — |
| p521 | 0.14.0 | — |
| pageant | 0.2.3 | — |
| password-hash | 0.6.1 | — |
| pkcs5 | 0.8.0 | — |
| pkcs8 | 0.10.2 | — |
| pkcs8 | 0.11.0 | — |
| poly1305 | 0.9.0 | — |
| portable-atomic | 1.15.0 | B |
| primefield | 0.14.0 | — |
| primeorder | 0.14.0 | — |
| pyo3 | 0.29.3 | B |
| pyo3-async-runtimes | 0.29.0 | — |
| pyo3-async-runtimes-macros | 0.29.0 | P |
| pyo3-build-config | 0.29.3 | — |
| pyo3-ffi | 0.29.3 | B |
| pyo3-macros | 0.29.3 | P |
| pyo3-macros-backend | 0.29.3 | — |
| quinn-proto | 0.11.16 | — |
| rand | 0.10.3 | — |
| ratatui | 0.30.2 | — |
| ratatui-core | 0.1.2 | — |
| ratatui-crossterm | 0.1.2 | — |
| ratatui-macros | 0.7.2 | — |
| ratatui-termwiz | 0.1.2 | — |
| ratatui-widgets | 0.3.2 | — |
| reqwest | 0.13.5 | — |
| rfc6979 | 0.6.0 | — |
| russh | 0.63.3 | — |
| russh-cryptovec | 0.62.0 | — |
| rustls | 0.23.45 | B |
| rustls-webpki | 0.103.15 | — |
| serde | 1.0.229 | B |
| serde_core | 1.0.229 | B |
| serde_derive | 1.0.229 | P |
| serde_json | 1.0.151 | B |
| serde_repr | 0.1.21 | P |
| serde_with | 3.24.0 | — |
| serde_with_macros | 3.24.0 | P |
| sha3 | 0.12.0 | — |
| signature | 2.2.0 | — |
| smallvec | 1.16.2 | — |
| spki | 0.7.3 | — |
| spki | 0.8.0 | — |
| ssh-cipher | 0.3.0 | — |
| ssh-encoding | 0.3.0 | — |
| strum | 0.28.0 | — |
| strum_macros | 0.28.0 | P |
| syn | 3.0.6 | — |
| thiserror | 2.0.21 | B |
| thiserror-impl | 2.0.21 | P |
| tokio | 1.53.1 | — |
| tokio-rustls | 0.26.6 | — |
| tokio-stream | 0.1.19 | — |
| tokio-util | 0.7.19 | — |
| toml | 1.1.6+spec-1.1.0 | — |
| toml_parser | 1.1.3+spec-1.1.0 | — |
| toml_writer | 1.1.2+spec-1.1.0 | — |
| tracy-client | 0.18.5 | — |
| tun-rs | 2.8.11 | B, OS FFI headers |
| webpki-roots | 1.0.9 | — |
| winreg | 0.56.0 | — |
| x25519-dalek | 3.0.0 | — |
| yoke | 0.8.3 | — |
| zerocopy | 0.8.59 | B |
| zerocopy-derive | 0.8.59 | P |
| zeroize | 1.9.0 | — |
| zeroize_derive | 1.5.0 | P |

</details>

<details>
<summary>44 new local package names, all version 0.57.4, source: workspace path</summary>

37 rename existing upstream packages; seven add implementations (`dns_wire`, `forwarder`, `host_net`, `magicsock`, `metrics`, `netmon`, `tka`). These `geiserx_` names are intentional fork branding, not hidden registry replacements in the reviewed checkout.

| Package name | Workspace location |
|---|---|
| geiserx_tailscale | `.` |
| geiserx_ts_array256 | `ts_array256` |
| geiserx_ts_bart | `ts_bart` |
| geiserx_ts_bart_packetfilter | `ts_bart_packetfilter` |
| geiserx_ts_bitset | `ts_bitset` |
| geiserx_ts_capabilityversion | `ts_capabilityversion` |
| geiserx_ts_control | `ts_control` |
| geiserx_ts_control_noise | `ts_control_noise` |
| geiserx_ts_control_serde | `ts_control_serde` |
| geiserx_ts_dataplane | `ts_dataplane` |
| geiserx_ts_derp | `ts_derp` |
| geiserx_ts_disco_protocol | `ts_disco_protocol` |
| geiserx_ts_dns_wire | `ts_dns_wire` |
| geiserx_ts_dynbitset | `ts_dynbitset` |
| geiserx_ts_elixir | `ts_elixir/native/ts_elixir` |
| geiserx_ts_ffi | `ts_ffi` |
| geiserx_ts_forwarder | `ts_forwarder` |
| geiserx_ts_hexdump | `ts_hexdump` |
| geiserx_ts_host_net | `ts_host_net` |
| geiserx_ts_http_util | `ts_http_util` |
| geiserx_ts_keys | `ts_keys` |
| geiserx_ts_magicsock | `ts_magicsock` |
| geiserx_ts_metrics | `ts_metrics` |
| geiserx_ts_netcheck | `ts_netcheck` |
| geiserx_ts_netmon | `ts_netmon` |
| geiserx_ts_netstack_smoltcp | `ts_netstack_smoltcp` |
| geiserx_ts_netstack_smoltcp_core | `ts_netstack_smoltcp_core` |
| geiserx_ts_netstack_smoltcp_socket | `ts_netstack_smoltcp_socket` |
| geiserx_ts_nodecapability | `ts_nodecapability` |
| geiserx_ts_overlay_router | `ts_overlay_router` |
| geiserx_ts_packet | `ts_packet` |
| geiserx_ts_packetfilter | `ts_packetfilter` |
| geiserx_ts_packetfilter_serde | `ts_packetfilter_serde` |
| geiserx_ts_packetfilter_state | `ts_packetfilter_state` |
| geiserx_ts_peercapability | `ts_peercapability` |
| geiserx_ts_python | `ts_python` |
| geiserx_ts_runtime | `ts_runtime` |
| geiserx_ts_time | `ts_time` |
| geiserx_ts_tka | `ts_tka` |
| geiserx_ts_tls_util | `ts_tls_util` |
| geiserx_ts_transport | `ts_transport` |
| geiserx_ts_transport_tun | `ts_transport_tun` |
| geiserx_ts_tunnel | `ts_tunnel` |
| geiserx_ts_underlay_router | `ts_underlay_router` |

</details>

<details>
<summary>Additional development-only fuzz lockfile inventory</summary>

The new `ts_tka/fuzz/Cargo.lock` has 67 packages: 47 match root name/version/source tuples, and the following 20 differ (18 registry tuples and two local packages). All 18 additional registry archive checksums matched this lockfile. [libfuzzer-sys](https://crates.io/api/v1/crates/libfuzzer-sys) is the one extra external name absent from both root inventories: version 0.4.13 has about 11.82M downloads; the crate dates to 2019-09-10. Its `build.rs` compiles bundled libFuzzer C++ via `cc`, or links operator-supplied `CUSTOM_LIBFUZZER_PATH`; no remote download/execute path was found. `thiserror-impl` is a proc macro; compiler probes/generation in `libc`, `proc-macro2`, `quote`, `thiserror` have expected build behavior. This harness is excluded from the production workspace and used explicitly by cargo-fuzz.

| Package | Version | Source / build note |
|---|---|---|
| arbitrary | 1.4.2 | crates.io registry |
| block-buffer | 0.12.1 | crates.io registry |
| cc | 1.4.4 | crates.io registry |
| cfg-if | 1.0.4 | crates.io registry |
| cpufeatures | 0.3.1 | crates.io registry |
| crypto-common | 0.2.2 | crates.io registry |
| find-msvc-tools | 0.1.11 | crates.io registry |
| geiserx_ts_tka | 0.46.1 | local path (stale independent fuzz lock version) |
| jobserver | 0.1.35 | crates.io registry |
| libc | 0.2.189 | crates.io registry; build script |
| libfuzzer-sys | 0.4.13 | crates.io registry; build script; native C++ |
| proc-macro2 | 1.0.107 | crates.io registry; build script |
| quote | 1.0.47 | crates.io registry; build script |
| shlex | 2.0.1 | crates.io registry |
| syn | 2.0.119 | crates.io registry |
| syn | 3.0.4 | crates.io registry |
| thiserror | 2.0.20 | crates.io registry; build script |
| thiserror-impl | 2.0.20 | crates.io registry; proc macro |
| ts_tka-fuzz | 0.0.0 | local fuzz harness |
| typenum | 1.20.1 | crates.io registry |

</details>

## Endpoints and connection inventory

Runtime destinations are either the configured control plane, its DERP/STUN/peer map, caller-configured traffic/backends, or the optional features below. Control/DERP may be non-Tailscale infrastructure with self-hosted configuration. Their trust comes from configuration/control, not a hard-coded hostname allowlist. No maintainer-owned runtime destination was found. Capability URLs are identifiers, not HTTP requests.

| Destination / port | Source location at reviewed commit | What is sent / classification |
|---|---|---|
| Default `https://controlplane.tailscale.com/`, TCP 443; configured control URL/port and control-provided dial-plan candidates | `ts_control/src/config.rs:8`; `ts_control/src/control_dialer.rs:196`; `ts_control/src/tokio/connect.rs:153` | Fine by default: public keys, registration auth key, HostInfo/system metadata, endpoints, routes, map polling. Noise authenticates/encrypts machine RPCs. HTTP bootstrap opt-in is finding 8. |
| Same control: `/machine/id-token`, `/machine/set-dns`, registration/logout and `/machine/tka/*` | `ts_control/src/tokio/id_token.rs:173`, `set_dns.rs:141`, `logout.rs:223`, `tka_mutation.rs:169`, `tka_sync.rs:144` | Fine destinations: audience/DNS records, public identity/AUMs/signatures; explicit TKA init/disable secrets go to control. Secret error previews: finding 4. |
| DERP hostname/fixed IP from control map; default TCP 443, configurable HTTPS port | `ts_derp/src/dial.rs:151,192`; `ts_control/src/derp.rs:49` | Fine: DERP protocol authentication with public identity/proof, encrypted WireGuard/disco traffic. No added private-key upload. Self-signed DERP uses SHA-256 pin + handshake-signature verification (`ts_tls_util/src/pinned.rs:96,103`). |
| STUN addresses from control DERP map, UDP 3478 or configured port; latency probes to mapped DERPs | `ts_runtime/src/direct.rs:868`; `ts_magicsock/src/sock.rs:2525`; `ts_netcheck/src/derp_latency.rs:84` | Fine: NAT binding requests reveal source IP/port; latency probes are operational, not analytics. No new independent public STUN host. |
| Peer/relay endpoint IPs and ports from netmap/authenticated disco; own underlay `0.0.0.0:<configured-or-dynamic-port>` or opt-in IPv6 | `ts_magicsock/src/sock.rs:103`; `ts_runtime/src/direct.rs:226`; `ts_magicsock/src/relay.rs` | Fine purpose: direct WireGuard/disco and negotiated peer relays. Dynamic network destinations, not hidden constants. |
| `https://api.tailscale.com:443/api/v2/oauth/token` and `/api/v2/tailnet/-/keys`; operator `baseURL` override | `ts_control/src/wif.rs:43,331,358` | Optional `identity-federation`: OAuth secret, API access token, requested tags. Override risk: finding 8. WIF token exchange instead uses configured control URL (`:465`). |
| `http://metadata.google.internal:80/computeMetadata/v1/instance/service-accounts/default/identity` | `ts_control/src/wif.rs:47,425` | Optional ambient WIF: requests an audience-scoped GCP JWT, then submits JWT to control. Expected non-Tailscale infrastructure, no default startup probe without federation inputs. AWS path reads `AWS_WEB_IDENTITY_TOKEN_FILE`; no AWS network endpoint is added. |
| `ACTIONS_ID_TOKEN_REQUEST_URL`, normally HTTPS 443 | `ts_control/src/wif.rs:382` | Optional ambient WIF: sends environment's GitHub OIDC request bearer token; caller/environment controls URL. |
| `https://acme-v02.api.letsencrypt.org:443/directory`, directory/order-provided ACME URLs | `ts_control/src/acme.rs:63,315,464` | Optional `acme`: node DNS name, public account JWK, signed JWS, CSR and challenge proof. Leaf/account private keys stay local; follows CA-provided URLs without an origin allowlist. Explicit low-level API can supply another directory. Fine intended CA traffic. |
| Tailnet peerAPI `http://<peer-ip>:<advertised-port>/dns-query`, `/v0/put/<name>`; tailnet DNS resolvers on UDP/TCP 53 or configured port | `ts_control/src/node.rs:531`; `ts_runtime/src/peerapi_doh.rs:188`; `taildrop_send.rs:180`; `magic_dns.rs:1631,1666` | Fine purpose: DNS/file transfer over overlay. No fixed public fallback resolver was added; resolvers come from control configuration. |
| SSH policy recorder `http://<policy-address>:<port>/v2/record` | `src/ssh/recording.rs:291,672,706` | Optional SSH: terminal output/session metadata to policy-selected recorder over tailnet dialer; not a hidden external recorder. Output can itself contain secrets. Required-recording failures gate shell startup (`src/ssh/shell.rs:436`). |
| Caller-specified tailnet TCP/UDP addresses/ports, local SOCKS targets | `src/lib.rs:604`; `src/loopback.rs:153`; `src/http.rs:169` | Fine API functionality. Tailnet HTTP connector explicitly rejects HTTPS schemes rather than transmitting plaintext to a TLS destination. |
| Operator proxy IP:port / peer-selected exit destination:port / advertised subnet destinations | `ts_forwarder/src/dialer.rs:105,153,242` | Expected non-Tailscale egress. Findings 2 and 5 apply; no hard-coded residential proxy/provider exists. |
| Configured Serve backend, including normalized `http://127.0.0.1:<port>` | `ts_control/src/serve/ipn.rs:541,550`; `ts_runtime/src/serve.rs:350,371` | Fine deliberate reverse-proxy/local service exposure; backend is chosen by configuration, not incoming HTTP authority. It uses host sockets outside proxy-egress path. |
| `127.0.0.1:<dynamic-port>` SOCKS5 and LocalAPI; bind placeholders `0.0.0.0`/`[::]` in overlay APIs | `src/loopback.rs:250`; `src/tsnet.rs:1314,1412` | Fine: authenticated local API/proxy. Wildcard overlay addresses are not host-wide management listeners. |
| `100.100.100.100:53`; `127.3.3.40:<DERP-region-ID>` | `ts_runtime/src/magic_dns.rs:147`; `ts_magicsock/src/sock.rs:166` | Fine internal MagicDNS service and DERP encoding sentinel; the latter is not a socket connection to that IP. |
| `169.254.169.254`; `100.64.0.0/10`, excluded ChromeOS `100.115.92.0/23`, `fd7a:115c:a1e0::/48`; split defaults `0.0.0.0/1`,`128.0.0.0/1`; synthetic `0.0.0.0`/`2000::` ACL probe | `ts_dataplane/src/lib.rs:20`; `ts_control/src/node.rs:38`; `ts_host_net/src/lib.rs:174`; `ts_runtime/src/peerapi_doh.rs:369` | Address classification/routing, not autonomous outbound requests. Metadata allow-exception matters in finding 2. Synthetic ACL probes are evaluated locally, not sent to internet. |
| `sh.rustup.rs:443`; Rust toolchain download hosts; crates.io/index/static registry; GitHub Actions/API/releases; cross container registry | `.github/actions/provision-self-hosted/action.yml:58`; `.github/workflows/ci.yml:157,316`; `release.yml:53`; `scripts/setup-trusted-publishing.sh:27` | Build/release infrastructure, not daemon phone-home. Registry tokens go to crates.io, PAT/GITHUB_TOKEN to GitHub. Mutable tooling: finding 7. |
| `localhost:14000`/`:15000` HTTPS, `localhost:8055` HTTP, `:8053` DNS; test configuration also names challenge ports 5001/5002 | `scripts/pebble-up.sh:24,99,119` | Development ACME services. Public binds/test keys: finding 9. |
| `raw.githubusercontent.com/{google,mozilla}/supply-chain/...`; PyPI/test-PyPI, Hex/staging Hex; `geiserx.github.io`, GitHub/crates/docs links | `supply-chain/config.toml`; `python.yml`; `elixir.yml`; `mkdocs.yml:4` | Vet imports, docs and language-package tooling. Python/Elixir build jobs are owner-gated off this fork; no new runtime uploader. Docs workflow installs pinned top-level MkDocs dependency, without a complete hashed dependency lock. |
| `https://tailscale.com/cap/funnel-ports`, `/cap/file-sharing`; `tailscale.com/cap/{file-sharing-target,web-client-url,resource-name,skip-username}` | `ts_control/src/node.rs:589,606,610`; `ts_control_serde/src/service_vip.rs:354` | Fine: protocol capability keys, never dereferenced as network destinations. |
| `example.*`, `*.example.ts.net`, `192.0.2.*`, `198.51.100.*`, `203.0.113.*`, fixture public resolvers (`8.8.8.8`, `1.1.1.1`, `9.9.9.9`) and assorted private/tailnet addresses/ports | Added unit/integration tests, examples and crypto-vector documentation | Test fixtures/documentation, not added production destination defaults. Live tests are explicitly gated (`TS_RS_TEST_NET`/auth key); they were not executed. |

**CI runner distinction:** despite repeated “self-hosted” comments, the active fork jobs actually specify `ubuntu-latest` (`ci.yml:63,271`); inherited custom runner labels are gated to owner `tailscale`. No active `runs-on: self-hosted` or `pull_request_target` workflow was found. There is **no same-repository PR gate** on those two active jobs despite comments claiming one. If runner configuration routes the `ubuntu-latest` label to persistent private/Docker-socket infrastructure, untrusted fork PR execution would be dangerous; actual runner registration was not accessible. Do not infer runner isolation from comments alone. Python's publish job has no owner check but depends on an owner-gated build, so normal fork publication is skipped.

## Coverage and what could not be checked

Used added-line diff triage plus ripgrep searches for URLs/hostnames/numeric and constructed IPs/ports, host/overlay clients, logging/serialization/Debug, filesystem writes/modes, build scripts/macros, subprocesses, `unsafe`, environment overrides, encoded/include payloads and date conditions. Read executable hits and their relevant callers in context; fixture/comment hits were grouped by purpose. All workflows/actions and four `scripts/*.sh` were inspected; root and fuzz dependency archives and manifest flags were screened without building them. This was targeted source review, **not line-by-line manual verification of every one of the 149k added lines**.

Could not establish:

- Correspondence between reviewed source and Kasper's running binary, deployed configuration, proxy endpoint, file permissions, enabled features or runtime egress. No packet capture, sandbox execution, build, live registration, exploit attempt, or test run was performed.
- GitHub branch/tag protections, actual runner environment, workflow permissions/settings, cache provenance, release signatures/content, crates.io ownership/trusted-publisher configuration, PAT scopes, or maintainer account security. No published binary/package-to-source comparison was made.
- Absence of malicious behavior in all 643 third-party packages, native assembly/prebuilt objects, unchanged upstream code, GitHub actions/toolchain binaries or their transitive downloads. The 162 added root and 18 additional fuzz external archives were checked/screened, not exhaustively audited; unchanged root external versions were not exhaustively inspected.
- A complete present-day vulnerability audit, every feature/target combination, cryptographic correctness, full SSH/TKA security parity, side channels, or concurrency/FFI memory safety. Very-new/download screening covered the 35 new crate names; all network metadata queries for those names succeeded. The independent fuzz lockfile was inventoried separately; it does not determine the production dependency graph.

The absence of an identified malicious payload supports a cautious “none found” verdict. The confirmed dangerous behaviors above prevent a stronger assurance.
