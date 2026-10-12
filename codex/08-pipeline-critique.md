Before I benchmark your pipeline-thread build, the user wants your independent judgement on whether the
premise holds. Please be adversarial; disagree where the evidence says so. Answer concisely in
docs/codex-08-pipeline-critique.md:

1. Interpretation of the measurements (BENCHMARKS.md §1c, tunnel-ab.csv, scratch/prof/sample*.txt):
   is "cross-thread wakeups/work-stealing across ~5 serial per-packet task hops" the right explanation
   for 1 worker = 3.6 CPU-s/GB vs 10 workers = 9.1? What else could explain it (e.g. the co-located Go
   client getting more CPU, macOS P/E-core placement, batch sizes growing when the node is slower, smoltcp
   poll frequency, time-driver/kevent costs)? Which of these can we distinguish with the data we have, and
   what extra measurement would settle it?
2. Expected gain of the dedicated pipeline thread vs 4 workers, single flow and 8 flows. Is 1-worker's 3.6
   a fair upper bound? Where would the dedicated thread fall short of it (packets still cross to the
   caller runtime for per-flow splices in exit-node mode; per-flow tasks wake the pipeline)?
3. Risks: one pipeline thread caps packet throughput at one core (~2 Gbit/s here) — acceptable for this
   home exit node, but say so; latency under load (can a burst on one flow delay others inside the
   single-threaded pipeline? smoltcp poll fairness); priority inversion with the security supervisors;
   blocking calls on the pipeline thread (sync locks held across sends, DNS?); shutdown.
4. Anything in your implementation you'd change after this review, and the exact build/run command for
   the A/B (env knob, binary path).
