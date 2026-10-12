Status: smoltcp stall fix (your patches) is merged and live; blocker 5 merged; everything pushed to
github.com/kasper0406/tailscale-rs branch ts-exit/runner (now 752cf20). New: a local Go tailscaled client
(bin/tailscaled, state/go-client) lets us benchmark the tunnel end to end on this Mac
(runner/tunnel-bench.sh, results bench/results/tunnel-ab.csv). Profiling the node under load
(scratch/prof/sample-{d,u}.txt, categorized by scratch/categorize.py) shows crypto at 15-19% of busy CPU,
and RustCrypto's soft poly1305 backend in the stacks. Microbench (bench/aead): RustCrypto
chacha20poly1305 0.10.1 seal+open 4077 ns per 1312-byte packet vs ring 0.17 1553 ns (2.6x).

Task: switch ts_tunnel's TRANSPORT-DATA AEAD (the per-packet seal/open in the session/endpoint data path)
to ring's CHACHA20_POLY1305, behind a cargo feature `ring-aead` on ts_tunnel (default ON is fine if you
judge it safe; keep the RustCrypto path compiling and tested with the feature off). Handshake AEAD may stay
RustCrypto unless switching it is trivial. Requirements:
 - Constant behaviour on the wire: same nonce layout (64-bit counter little-endian after 4 zero bytes),
   empty AAD, tag placement, padding, error handling (a failed open must not leak/retain plaintext and must
   behave identically for replay/counter logic).
 - Key material hygiene equivalent to today (zeroize on drop where we do now; ring's LessSafeKey can't be
   zeroized — document the trade-off honestly and keep the raw key bytes zeroized).
 - No extra per-packet copies or allocations versus today (ring seals in place with a separate tag;
   open_in_place needs ciphertext||tag contiguous, which the WireGuard packet already is).
 - Tests: run the existing Wycheproof ChaCha20Poly1305 vectors against the ring path too; existing
   ts_tunnel tests must pass with the feature on and off; add a cross-test that RustCrypto-sealed packets open
   with ring and vice versa.
 - Measure: ts_tunnel unit benches if any, plus the per-packet ns in the data path before/after.
Deliver format-patches in scratch/codex-ring-aead/ + README; DCO `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`.
Base on ts-exit/runner 752cf20 in your own worktree; CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target.
Do not touch the live node, the runner checkout, or the Go client.
