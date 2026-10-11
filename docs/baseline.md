# Baseline: environment, pinned sources, build

Recorded 2026-10-11.

## Host

| | |
|---|---|
| Machine | Mac mini, Apple M4 (10 cores), 16 GiB |
| OS | macOS 26.6.2 (25G83), Darwin 25.6.0 arm64 |
| Network | en0 (Ethernet) 192.168.50.76/24 behind the home ASUS router; **no global IPv6** (en0 has only `fe80::`, no IPv6 default route) |
| Official Tailscale on this Mac | not installed (no app, no `tailscaled`) |
| Go toolchain | not installed; Go baseline declined (DECISIONS D2) |

## Pinned repositories

All cloned under `/Volumes/git/ts-exit/repos/` (ignored by this repo's git).

| Repo | Remote | Branch | HEAD |
|---|---|---|---|
| official Rust | https://github.com/tailscale/tailscale-rs | main | `b44db0da2dc2cc4becd58d1337e81267cf426e17` (2026-10-07) |
| fork (functional reference) | https://github.com/GeiserX/tailscale-rs | main | `719aa8891fd777daafab67700d3d05894e49b71f` (2026-10-10) |
| daemon layer (inspected only) | https://github.com/GeiserX/tailscaled-rs | main | `f95330a6a9b18489b4311d08640d20a60096a6ff` (2026-10-10) |
| Go reference | https://github.com/tailscale/tailscale | main | `3bb8a906c27738c7904ef301c9539da763181f85` (2026-10-10), blobless clone |

The fork shares **no git history** with the official repo (`git merge-base` is empty).

Work branch: fork `ts-exit/runner`, based on `719aa889`. Patch series: `git log 719aa88..ts-exit/runner`.

## Toolchains

| | |
|---|---|
| official repo | `rust-toolchain.toml` pins 1.98.1 (rustc 1.98.1 48a229cea 2026-09-01) |
| fork | `rust-toolchain.toml` pins 1.95.0 (rustc 1.95.0 59807616e 2026-04-14), edition 2024 |
| rustfmt | nightly 2026-10-10 (`cargo +nightly fmt`, as CONTRIBUTING.md asks; reverted drift-only reformatting of files we didn't touch) |
| Target | `aarch64-apple-darwin`, release profile |

## Build

```sh
# both workspaces build clean in release mode (~2 min each from cold)
cd repos/tailscale-rs && cargo build --release --workspace
cd repos/geiserx-tailscale-rs && cargo build --release --workspace

# the exit-node runner (fork branch ts-exit/runner)
runner/build.sh      # = cargo build --release --locked --example exit_node
```

Binary: `repos/geiserx-tailscale-rs/target/release/examples/exit_node`.
Requires `TS_RS_EXPERIMENT=this_is_unstable_software` at run time (set by `runner/start.sh`).

## Test baseline (no tailnet)

At the fork's unmodified `719aa889`: `cargo test -p geiserx_ts_forwarder` → 52 passed.
At `ts-exit/runner`: ts_forwarder 70 passed (40 unit + 6 + 15 + 3 + 5 + 1 bench-free), ts_runtime 581,
ts_control 322, tailscale (root lib) 28: all pass.

## Live baseline

Pending: needs the tailnet auth key (`~/.config/ts-exit/authkey`) and route approval. Direct-vs-DERP path
evidence is logged by the runner every 30 s (`peer ... path=direct <ip:port>` or `path=derp <region>`) and
must be quoted with every live result. See RUNBOOK.md.
