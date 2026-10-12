# Pipeline premise: independent critique

**Worth testing, but “five cross-thread hops explain 61%” and “3.6 bounds the gain” are overstated. Keep this opt-in.** Reviewed §1c, CSV, six profiles/categorizer and implementation `ae21eff`; no live action performed. New A/B rows appeared during the review and are distinguished below.

## 1. Evidence versus explanation

Historical download medians: **4 workers: 8.97 CPU-s/GB at 1,800 Mbit/s; 2: 6.42 at 1,940; 1: 3.56 at 2,063** (`tunnel-ab.csv:82–105`). About 2.02, 1.56 and 0.92 Rust cores. Upload with one worker slows to 1,719 Mbit/s. Faster completion alone cannot explain ~60% cheaper CPU/GB with only ~15% faster download.

Cross-thread scheduling/cache coherence is a strong candidate, not an isolated cause. These are **task/channel hops**, not five Kameo actor messages per packet (`ts_runtime/src/netstack_actor.rs:203`, `dataplane.rs:300`, `direct.rs:634`, `ts_magicsock/src/sock.rs:2936`). Direction/connected receive alter the path; a hop need not cause a kernel wakeup or steal.

Profiles cover successive binaries, **not matched worker counts**. `sample4-d.txt:33–10784` shows ten workers; categorized shares are Tokio 11.0%, locks/wakeups 8.1%, clock 5.1%, allocation/copy 14.9%. This does not establish 61% removable scheduling cost; the patch retains channels and allocations. `sample` includes blocked stacks; `categorize.py:2,8,24` excludes kevent/waits and classifies TLS lookup as locks/wakeups. Its “busy %” is not calibrated process CPU attribution. CV waits show sleeping; no measured steal count is supplied.

| Alternative | What is known; discriminating measurement |
|---|---|
| Co-located Go / shared kernel | Rust accounting excludes Go, but Go changes ACK timing, contention and batching. Record both processes' CPU and wire packets; repeat with Go on another host. Faster Go alone cannot explain the cost ratio. |
| P/E cores, frequency/cache locality | No residency evidence. A dedicated OS thread **is not CPU affinity**. Obtain matched per-thread on-CPU/core-residency/context-switch traces. CPU seconds are not equal-speed cycles. |
| Batching / smoltcp poll frequency | Earlier coalescing demonstrably helped. One-worker download is faster, so “slower node” alone is insufficient; internal stages can still run in larger bursts. Count batch sizes, polls/commands per packet, retransmits and queue peaks. |
| Time driver / kevent / parking | Stacks show presence, not events/GB or cost. Count reactor turns, timer wakes, parks/unparks and migrations/steals. This patch adds a second reactor/time driver. |

Worker-3 and early ring-batched downloads have major outliers. Historical CSV lacks path/flag histories and raw eight-flow rows supporting §1c. Use randomized/reversed pairs, warm-up, fixed offered rates (750 Mbit/s) and saturation. **Remote Go + matched rate + batch/poll/wakeup counts** best separates causes. Sample instrumentation rather than restoring per-packet tracing.

## 2. Expected gain; newly available results

My pre-A/B planning prior was **0–30% CPU saving for one download, 0–25% for eight flows**, with regression possible—guesses, not bounds. The older four-worker table showed no stalls; a major p99 improvement over that baseline was not assured.

**Appended during review** (`tunnel-ab.csv:106–119`, `bench/results/latency.txt`): single download median CPU **8.88 → 4.67** (~47% cheaper), throughput **1,815 → 2,000**; upload CPU **9.87 → 6.52**, throughput **1,946 → 1,799**. CPU savings exceed my conservative prior. But eight-flow throughput **1,043 → 642 Mbit/s** (~38% regression), CPU **14.42 → 7.69**; echo p99 **18.75 → 14.52 ms**, max **3,021.61 → 14.67 ms**. A’s new 3 s maximum also contradicts “four workers are always safe.” One unrandomized pair cannot establish success. Assuming full 4 GB, B averages **0.62 Rust cores** over ~50 s: continuous one-core saturation alone cannot explain it. Inspect flow tails, loss/retransmissions and Go CPU. B's probe lasts at most ~3.7 s plus startup sleep, so it cannot dominate that interval.

**3.6 is neither a rigorous cost floor nor a savings ceiling.** The one-worker run co-locates everything and stalls; this patch retains queues/locks/allocations and cross-runtime commands/completions. Exit splices/UDP relays remain caller tasks (`forwarder_actor.rs:70`, `ts_forwarder/src/tcp.rs:91,udp.rs:236`). Even tunnel-only sink/echo handlers remain caller tasks (`examples/exit_node/main.rs:192–194,273`); their wakeups and Go-side latency survive isolation.

## 3. Risks

- **Capacity:** all selected stages share one core, losing previous overlap. Roughly 2 Gbit/s down / 1.7 up is the observed whole-node one-worker ceiling, not a calibrated pipeline limit. Acceptable for this ~750 Mbit/s home bulk exit with headroom; small packets and duplex load can exhaust it sooner.
- **Fairness/security:** biased I/O selection and unbudgeted drains (`ts_netstack_smoltcp/src/run.rs:68`; `ts_netstack_smoltcp_core/src/lib.rs:196,439,587`) let bursts delay other flows, timers and **same-thread SecurityGuard monitors** (`security.rs:62`). Abort cannot preempt synchronous work. Bus/filter/source-filter/peer-tracker writers still contend with caller bulk tasks. Tests prove caller-hog isolation and eventual cancellation, not bounded flood/revocation response.
- **Blocking:** UDP sends release std mutexes before awaiting (`sock.rs:504,1439`). Cold socket syscalls hold the connected-table lock (`connected.rs:148`); gate flush holds its mutex (`port_gate.rs:185`). DataPlane's async core lock spans output helpers, but these only enqueue unbounded messages (`async_tokio.rs:235,304–321`). Maintenance contention, large updates and inline capture hooks can still stop the lane.
- **Shutdown:** DERP UseDns executes resolver work off-thread (`ts_derp/src/dial.rs:205`), but destruction waits for it (`pipeline.rs:47,79–99`). A resolver stall can exceed the async shutdown timeout; joining blocks a caller worker. Self-drop delegates joining to the caller blocking pool, requiring that runtime alive. Stuck-resolver teardown is untested.

## 4. Changes and reproducible A/B

No placement change before the diagnostic A/B. Before default-on, I would **budget/yield command and I/O drains**, test flooded-pipeline echo/failure/revocation latency, add sampled counters, and move DERP resolution to the caller while keeping socket registration dedicated. Stage fusion should follow measured channel costs.

`ab-matrix.sh:10–14` measures eight downloads only, includes probe duration in elapsed time and discards client byte counts. Record actual bytes/per-flow completion, delimit probe load, and add eight uploads. The new throughput regression merits investigation before adoption.

Reproduce the build (already exported):

```sh
cd /Volumes/git/ts-exit/scratch/wt-codex
CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target \
  cargo build --profile exitnode --locked --example exit_node --features ring-aead
cp /Volumes/git/ts-exit/scratch/codex-target/exitnode/examples/exit_node \
  /Volumes/git/ts-exit/scratch/codex-pipeline-thread/exit_node
```

Same-binary off/on pair, four caller workers, explicit default 256 KiB buffers. **Commands restart the benchmark node; for Claude to execute:**

```sh
cd /Volumes/git/ts-exit
for mode in 0 1; do
  RUNNER_ARGS="--worker-threads 4 --tcp-buffer-size 262144" \
    runner/ab-matrix.sh "pipeline-4w-$mode" \
    /Volumes/git/ts-exit/scratch/codex-pipeline-thread/exit_node \
    TS_EXPERIMENT_PIPELINE_THREAD=$mode \
    TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1 \
    TS_DEBUG_DONT_FRAGMENT=1 TS_DEBUG_MAGICSOCK_BATCHED_IO=1
done
```

Reverse order and repeat; then sweep caller counts 2 and 1. If the original workload used 2 MiB, set `2097152` in **both** arms. Cheaper CPU with unacceptable throughput, drops or stalls fails the experiment.
