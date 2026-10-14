# Review 28: validate the deployment design before switching the live exit node (read-only)

Kasper runs a Rust Tailscale exit node on his home Mac mini (M4, macOS, FileVault on, reachable only remotely;
he cannot physically reboot it for weeks). It serves his own laptop and iPad. Constraints that must hold: the
node never runs as root and never changes the host's routes, DNS, packet filter or TUN; no auth keys in files,
logs or history; no public IPs committed. Workspace: `/Volumes/git/ts-exit` (`RUNBOOK.md`, `docs/DECISIONS.md`,
`docs/UPSTREAM.md`). **Do not modify code, branches or the live system; write only your report.**

## What is about to change

1. **Code base switch.** GeiserX/tailscale-rs merged all our PRs (#597 exit node, #598 performance, #599–#614 fixes)
   into `origin/main` (18fc6bc, repo `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`), but excluded our vendored
   smoltcp TCP loss-recovery patches (SACK/NewReno/Cubic fixes/pacing/200 ms RTO floor) because GeiserX moved to
   crates.io smoltcp 0.14. Plan: run **GeiserX main + one commit** (`0e10bc3` on branch `ts-exit/next`,
   worktree `/Volumes/git/ts-exit/scratch/wt-gx`: `--serve-tcp PORT=ADDR`, Tailscale Serve `TCPForward` to a
   loopback backend), instead of our layered fork (`ts-exit/main` fcb6dbe = our vendored smoltcp 0.13.1 + patches).
   Our old layers stay as history; future work = upstream the TCP recovery to smoltcp-rs/smoltcp, and the serve
   flag as a GeiserX PR.
2. **Daemon settings** (`deploy/launchd/run.sh`, the draft now in the workspace, not yet installed): hostname
   `kmini` (renamed from `rust-exit` already), `--serve-tcp 22=127.0.0.1:22` so `ssh kmini` reaches the Mac's
   sshd, the TCP window back to the default 256 KiB (was 1 MiB with our patched smoltcp), and the now opt-in
   experiments enabled: `TS_EXPERIMENT_RX_BATCH=1 TS_EXPERIMENT_SPLICE_COALESCE=1 TS_EXPERIMENT_TARGETED_REPLAY=1`
   plus `TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1 TS_DEBUG_DONT_FRAGMENT=1 TS_DEBUG_MAGICSOCK_BATCHED_IO=1`.
3. **launchd daemon** (already installed and running): `deploy/launchd/` (`dev.kasper.rust-exit.plist`, `run.sh`,
   `rotate.pl`, `install.sh`, `update.sh`, `uninstall.sh`; RUNBOOK "Permanent setup"). System daemon with
   `UserName`, state and binary in `~/Library/Application Support/rust-exit`, logs in `~/Library/Logs/rust-exit`.

## Questions (answer each with evidence: file:line, upstream behaviour, or a concrete scenario)

1. **Is switching to GeiserX main sound?** Diff `ts-exit/next` against our `ts-exit/main` and list what we lose
   or gain beyond the TCP recovery (e.g. GeiserX's follow-up commits on #597/#598: exit DNS changes,
   connected-socket bounds, Linux buffers; anything of ours silently dropped). Any regression risk for this
   deployment? Is 256 KiB the right window on stock smoltcp 0.14 (with the `socket-tcp-cubic` feature they
   enabled), given `docs/DECISIONS.md` F31/F34/F35 and `docs/TCP-RECOVERY-24.md`?
2. **Experiments on GeiserX main:** GeiserX made them opt-in and wrote the PR has "no A/B of the default build".
   Are they safe to enable here (correctness, memory/CPU bounds, interaction with GeiserX's follow-up fixes such
   as the connected-socket demux bounds)? Should any stay off?
3. **`--serve-tcp` security and behaviour** (`examples/exit_node/main.rs` in the worktree, `src/lib.rs`
   `set_serve_config`, `ts_runtime` serve code): who can reach port 22 (packet filter/ACL enforcement on the serve
   listener?), does the all-port exit forwarder ever intercept traffic addressed to the node's own IP, what sshd
   sees as the source (127.0.0.1) and whether that weakens anything on macOS (e.g. `sshd` config, PAM,
   `AllowUsers`, rate limiting), behaviour when sshd is down, and whether loopback-only validation is enough.
4. **launchd design:** correctness of the plist (UserName daemon before login, KeepAlive/ThrottleInterval,
   ProcessType, Umask 077, NumberOfFiles 61440, HOME), the zsh wrapper's signal forwarding and the Perl rotator
   (data loss, blocking the node if the rotator dies, disk exhaustion), FileVault/`fdesetup authrestart`, sleep
   settings, the external USB volume, and update/uninstall safety. Anything that could leave the remote Mac without
   the node after a reboot or crash, and how to detect it remotely.
5. **Naming:** renaming the node to `kmini` (the Mac's hostname) and offering the exit node under that name. Any
   pitfalls (MagicDNS collisions, clients' exit-node selection, key expiry, tags/ACLs)?
6. **The plan going forward:** upstream the TCP recovery to smoltcp (porting onto smoltcp main), the serve flag to
   GeiserX, and keep only benchmark tooling locally. Is there a better path (e.g. carrying the smoltcp patches as a
   `[patch.crates-io]` on GeiserX main in the meantime, and is that worth it for a ~25 ms, 50–250 Mbit/s Wi-Fi
   client)?

Write `/Volumes/git/ts-exit/docs/review-28-design.md`: verdict first (go / go with changes / no-go, with
confidence), then findings by severity with concrete fixes, then answers to the six questions. Concise.
