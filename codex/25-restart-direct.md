# Brief 25: after a node restart, an already-connected peer stays on DERP from the node's side

## Symptom (live, 2026-10-13, `docs/DECISIONS.md` F31 item 4)

Exit node `examples/exit_node` from `ts-exit/main` (repo `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`,
GeiserX/tailscale-rs fork plus our layers) on a home Mac, behind NAT, fixed UDP port 41642. Client: a laptop
running the official Go Tailscale macOS app, away from home behind its own NAT.

- The laptop joins while the node is running: the node's status line shows `path=direct` within seconds.
- Restart the node process (same state dir, same node key; disco key is regenerated at start): from then on, the
  node's status line for the laptop is `path=derp fra`, for at least 15 minutes (observed four times, with the
  committed build and with experimental builds alike).
- Meanwhile, the laptop's own `tailscale status` and `tailscale ping rust-exit` say **direct** (pong via the
  node's public ip:41642 in 23 ms). So laptop→node goes direct while node→laptop uses DERP.
- Quitting and reopening the Tailscale app on the laptop (which changes its endpoints and disco key in the
  netmap) makes the node go `path=direct` within ~10 s.
- Node log lines around restarts: `learned peer disco key from a TSMP advertisement` (once, ~11 s after one
  restart), repeated `ts_tunnel::endpoint: handshake replay detected, bailing out`, and once `ts_runtime::direct:
  peer disco key changed; dropping the direct path built under the old key`.
- Earlier, in the synthetic-peer harness, we saw the related behaviour "the node only probes a peer endpoint when
  it appears or changes in the netmap, and control remembers endpoints across sessions" (`docs/DECISIONS.md`
  D29), and worked around it by rotating peer ports.

Go's magicsock keeps re-probing while it sends over DERP (`endpoint.send` → `sendDiscoPingsLocked` when there's
no trusted best address; `heartbeat`; `handlePingLocked` adds unknown sources as candidates;
`handleCallMeMaybe`). A stock Go node recovers from this within seconds.

## Task

1. Find the root cause with evidence. Start from `ts_runtime/src/direct.rs` (pinger, reconcile, the "rotated
   peers" re-probe), `ts_magicsock/src/sock.rs` (`send_pings`, `candidates_to_ping`, ping/pong handling, CallMeMaybe)
   and `ts_magicsock/src/path.rs` (cadence gates: 5 s discovery floor, good-enough quiet-down, 60 s upgrade
   re-probe, trust). Questions to answer: after a restart, does the node ping the laptop's netmap endpoints at all?
   With which disco key, and does the laptop still hold the node's *old* disco key at that moment, so it drops
   them? After a failed early round, is the peer's candidate set silenced or backed off until the netmap
   changes? Does an inbound ping from the laptop (which the node does answer) seed or confirm a candidate the
   way Go's does? Does the node ever send CallMeMaybe over DERP? What is different when the laptop is new?
2. Reproduce it deterministically in a test (magicsock- or runtime-level, with simulated peers and a simulated
   node restart, i.e. a new disco key with the peer's state kept), failing before the fix.
3. Fix it to match Go's behaviour, on a new branch `fix/restart-direct` cut from `ts-exit/fixes` (use a worktree
   under `/Volumes/git/ts-exit/scratch/`). Keep the fix small and in Go's terms. Also check it against GeiserX
   `origin/main`; if the bug is in GeiserX's code, the fix must apply there too.
4. Write a hand-off script `/Volumes/git/ts-exit/runner/restart-direct-check.sh` that Claude can run against the
   live node: restart the node, then poll its status lines for one named peer and report how many seconds it took
   to reach `path=direct` (time out after 120 s). Read the existing `runner/*.sh` and `runner/detach.py` for how
   the live node is started; don't run it yourself.

## Constraints

- Don't start, stop or touch the live node or its state dir (`/Volumes/git/ts-exit/state/rust-exit`), and don't
  join the real tailnet. Don't push or touch remotes.
- Commits: `Signed-off-by: Kasper Nielsen <kasper0406@gmail.com>` and `Co-Authored-By: GPT-6.1 (codex)
  <noreply@openai.com>`. No public IP addresses anywhere (use 203.0.113.0/24 in tests).
- Full tests for the crates you touch, clippy and fmt clean. Use `nice` and at most 4 build jobs.

## Deliverable

`/Volumes/git/ts-exit/docs/RESTART-DIRECT-25.md`: root cause with code references, the test, the fix, whether
GeiserX `origin/main` and official tailscale-rs (remote `official`) have the same bug, and the hand-off script's
usage. Concise.
