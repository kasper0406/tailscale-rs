# Brief 20: make RX batching and splice coalescing wins on Linux too (without losing the Mac gains)

Worktree `/Volumes/git/ts-exit/scratch/wt-codex-linux`, branch `ts-exit/linux-fixes` (from `ts-exit/consolidated`
@ 3e9ee27). Same rules as before: small reviewable commits with the usual trailers, `cargo +1.95.0 --locked`,
touched-file nightly rustfmt, no push. Priority (Kasper): macOS performance is primary; Linux must not regress,
and ideally gains too.

Note: all ts-exit/* branches were history-rewritten today (a public IP scrub); hashes in older briefs/docs were
remapped. Never put public IPs in code or tests; use 203.0.113.0/24 for sample public addresses.

## Evidence: Linux, kworkstation (Ryzen 9 9950X3D), node pinned to 8 physical cores of one CCD, W=8, N=1

Per-feature attribution (generator/sink on the other CCD; two passes for OFF/ALL; docs/DECISIONS.md F28):

| load/dir | OFF Mb/s, s/GB | ALL | RX_BATCH only | SPLICE_COALESCE only | TARGETED_REPLAY only |
|---|---|---|---|---|---|
| 1×1 down | 4733, 4.62 | 4082 (−14%), −18% CPU | 4684 (≈) | **4049 (−14%)**, −18% CPU | 4688 (≈) |
| 1×1 up | 4064, 4.81 | +4%, −23% CPU | +3%, −19% CPU | ≈ | ≈ |
| 2×8 down | 3925, 5.06 | +4%, −17% CPU | **−10%**, +4% CPU | +4%, −10% CPU | +3% |
| 2×8 up | 4647, 5.17 | **−28%** | **−18%** | −3% | −7% (overlapping) |
| 2×64 down | 3347, 6.17 | +5% | −8% | +1%, +15% CPU | +17.5%, −10% CPU |
| 2×64 up | 4752, 6.21 | −22% (noisy) | −27% (noisy) | −12% (noisy) | ≈ |

With ALL on, the node used ~2 cores vs ~3 with OFF on the same loads (more efficient per byte, less parallel).
On macOS (shared, contended cores) both features are clear wins: coalescing −31% CPU/GB on 1-flow download,
RX batching −17% on 2×64 upload (BENCHMARKS §5, F24, F26).

Current state on `consolidated` 3e9ee27: RX_BATCH and SPLICE_COALESCE default on only on Apple; the env vars
force either way. Keep that default until a fix is proven on both platforms.

## Hypotheses to verify, then fix

1. **RX batching serializes decrypt/dispatch.** With batching, a receive pump hands a whole batch (up to 16
   datagrams, bridge chunks of up to 64) to one task, so WG decrypt + netstack admission for the batch run on
   one worker; without batching, per-datagram work spreads across Tokio workers. Check where batches are
   consumed (`ts_magicsock` receive pumps, `ts_runtime/src/direct.rs` demux, `netstack_actor` admission) and
   whether the per-peer decrypt is now single-threaded per batch. Fix idea: keep the batched syscall
   (`recvmmsg`/`recvmsg_x`) but fan the decrypt/admission work back out (e.g. split batches per peer/flow
   across workers, or cap the per-task batch handed downstream) so syscall savings stay and parallelism returns.
   Also explain why RX alone hurts downloads (ACK receipt latency? fewer, burstier ACK deliveries → slower
   window growth?).
2. **Coalescing adds latency on a fast host socket.** The splice gathers ready bytes up to 128 KiB before one
   host write. On Linux's fast loopback/host TCP, holding data delays ACK/window progress for a single flow.
   Fix idea: bound the hold by bytes and by "no more data immediately ready", never by waiting; write
   immediately when the host socket is writable and the buffer is above a small threshold; or coalesce only
   the overlay→host direction, or only when the previous write was partial/pending. Measure write-size
   histograms (you added counters in brief 18) to confirm.

## Deliverables

- Code fixes behind the existing flags (so Claude can A/B on kworkstation with the same binary), tests for any
  new batching/splitting logic and for unchanged semantics (half-close, FIN, ordering, fairness).
- A local in-process probe comparison if useful (your brief-18/19 probe), but the authoritative numbers come
  from kworkstation; give Claude an exact small A/B matrix (OFF vs fixed RX vs fixed COAL vs both, W=8 N=1,
  1×1/2×8/2×64 both directions, 2 passes).
- If a fix is a clear Linux win in your reasoning and neutral on macOS, propose changing the default to on
  everywhere (but don't change defaults yourself).
- Full test suite as in briefs 15–19, both with the flags unset and forced on (`=1`) and forced off (`=0`).
- `docs/codex-20-linux-fixes.md` with evidence, root cause, fixes, tests and the A/B matrix.
