New bug found live, reproduced in-process. Please root-cause and fix.

Live: rust-exit (release, ts-exit/runner 5d6b650, --tcp-buffer-size 2097152) forwarding a 50 MB HTTPS download to a
Go Tailscale 1.88 client over a direct path (~23 ms RTT, phone hotspot). Host socket to Cloudflare had received 45.6 MB
and then 1.35 MB sat unread in its Recv-Q forever: the overlay TCP leg (smoltcp forwarder socket -> client) stalled.
With the default 256 KiB buffers the same test ran at ~75 Mbit/s.

Reproducer (forwarder bench now has LOSS_PCT; uncommitted on the runner checkout's working tree — it's in
repos/geiserx-tailscale-rs/ts_forwarder/benches/forward_throughput.rs; copy it into your worktree):
  FWD_MODE=gated RTT_MS=25 LOSS_PCT=2 TCP_BUF_KB=2048 cargo bench -p geiserx_ts_forwarder --bench forward_throughput -- 15 1 down
hangs (never completes); 256 KiB at 2% completes (23 Mbit/s); 2 MiB at 0.5% completes (296 Mbit/s).
Results: bench/results/forwarder-loss.txt. Note both ends of the bench are smoltcp; in the live case only the sender
(forwarder) side was smoltcp, the receiver was macOS kernel TCP, so the sender-side recovery is the prime suspect.

Task:
1. Root-cause with evidence (smoltcp 0.13.1 source in ~/.cargo/registry; the fork's netstack wrapper
   ts_netstack_smoltcp_core). Candidates: RTO/backoff handling with large in-flight, retransmit from snd_una with
   window scaling, zero-window/persist, timer starvation in the netstack poll loop (does the wrapper poll smoltcp
   when only a timer is pending?), or the WakingPipe/ingress queue dropping under bursts. Instrument as needed.
2. Fix it at the right layer (prefer the fork's wrapper; if smoltcp itself is buggy, a minimal vendored patch or
   a wrapper-side workaround, with a note on upstreaming). Add a regression test (bench-style or integration test with
   loss) that hangs before and passes after, bounded in time.
3. Report throughput for RTT 25 ms × loss {0, 0.5, 2, 5}% × buffer {256K, 2M, 4M} before/after, 1 flow down and up.
Deliver format-patch(es) in scratch/codex-smoltcp-stall/ + README, DCO `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`.
Use CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target and your own worktree; don't touch the live node or
the runner checkout.
