"""Summarize stage0-matrix.sh output: per config/direction, median Mbit/s, node CPU-s/GB, generator CPU-s/GB, flow spread."""
import json, statistics, sys, pathlib
for d in sys.argv[1:]:
    d = pathlib.Path(d); print(f"## {d.name}")
    for f in sorted(d.glob("*.jsonl")):
        rows = [json.loads(l) for l in open(f) if l.strip().startswith("{")]
        for direction in ("down", "up"):
            r = [x for x in rows if x["direction"] == direction]
            if not r: continue
            med = lambda k: statistics.median(x[k] for x in r)
            secs = [fl["seconds"] for x in r for p in x["peers"] for fl in p["flows"]]
            gen = statistics.median(x["generator_cpu_s"] / (x["bytes"] / 1e9) for x in r)
            print(f"{f.stem:8} {direction:4} n={len(r)} {med('mbit_s'):7.0f} Mbit/s  node {med('node_cpu_s_per_gb'):5.2f} s/GB  gen {gen:5.2f} s/GB"
                  f"  flow s min/med/max {min(secs):.2f}/{statistics.median(secs):.2f}/{max(secs):.2f}"
                  f"  [{', '.join(f'{x['mbit_s']:.0f}' for x in r)}]")
    for t in sorted(d.glob("rtt-*.txt")): print(f"{t.stem}: {t.read_text().strip()}")
