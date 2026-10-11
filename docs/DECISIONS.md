# Decision / issue log

Newest last. Each entry: date, decision or finding, evidence, consequence.

## 2026-10-11

- **D1 Workspace.** All work lives in `/Volumes/git/ts-exit` (git repo; `repos/`, `state/`, `scratch/` ignored).
  Upstream clones are under `repos/` and pinned in `docs/baseline.md`.
- **D2 No Go baseline.** User declined installing Homebrew `tailscale` on the Mac, so the "Official Go Tailscale exit node"
  row of the A/B table is dropped. Comparisons are Rust-vs-Rust only. Environmental ceilings (ISP up/down, remote link) are
  still measured.
- **D3 Test peers.** iPad (official client) plus a remote test laptop (official client, outside the home LAN). No public VPS,
  so the WAN egress target has to be a public endpoint the laptop can reach through the exit node
  (see BENCHMARKS.md for which one and how its stability is controlled).
- **D4 Auth.** Tagged (`tag:rust-exit`), reusable, short-expiry key in `~/.config/ts-exit/authkey` (0600 in a 0700 dir).
  Read from the file only on first registration; never passed on argv or env, never logged.
- **F1 Home network has no global IPv6** (en0 has only `fe80::`; no IPv6 default route). IPv6 egress is impossible
  from this Mac regardless of software, so `::/0` must not be advertised. What remains is client-side behaviour:
  see the IPv6 section of `docs/phase1-validation.md`.
- **F2 Fork and official repo share no git history** (`git merge-base` is empty). The fork is a re-import plus 622 commits,
  so "cherry-pick" into official is not possible. Track B ports have to be manual, file by file.
- **F3 Official contribution policy** (CONTRIBUTING.md @ b44db0da): "Currently, we are not requesting external code or docs
  contributions ... contact the maintainers (e.g., by filing an issue) before sending a PR." DCO sign-off is required.
  So the right step is an issue/discussion first, not a PR. Track B is therefore kept to a patch series on a local branch.
- **D5 Runner.** Use a ~150-line purpose-built runner (`examples/exit_node` on fork branch `ts-exit/runner`) instead of
  `tailscaled-rs`. It needs nothing from the daemon layer (IPC, launchd, persisted prefs), and keeping it in-tree
  reuses the fork's `Cargo.lock`.
