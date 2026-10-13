# kworkstation (Linux) scaling + regression matrix, 2026-10-13

Ryzen 9 9950X3D, Ubuntu 24.04, Linux 6.8. Node pinned to the frequency CCD (B4 = CPUs 8-11, B8 = CPUs 8-15,
SMT siblings idle). Generators, hold sessions and the sink are on the V-Cache CCD (0-7 and 16-23). Node binary
from `ts-exit/shard-stage1` (ace33ee node code), sha 65058c94ee5e. Generator f5505cdd4c1a (729ce69) for B4-* and
OFF, 14069d1a5e72 (7dfd60b) for the other B8 configs; the two differ only in post-measurement teardown.
ON = TARGETED_REPLAY + RX_BATCH + SPLICE_COALESCE (+ SHARD_BATCH when N>1). OFF = none. Generator workers = 4.

Raw compact rows (one per transfer) are kept on kworkstation in `~/ts-exit-bench/results/compact-rows-B.txt` (`cfg|load|run|dir|bytes|mbit_s|node_cpu_s|node_s_per_gb|
xfer_s|gen_cpu_s|gen_bin|flow_seconds_min/med/max`). The full per-flow JSONL stays on kworkstation
(~/ts-exit-bench/results). Analysis: docs/DECISIONS.md F28.
