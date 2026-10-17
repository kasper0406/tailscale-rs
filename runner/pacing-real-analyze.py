#!/usr/bin/env python3
"""usage: pacing-real-analyze.py <node-stats.jsonl> <laptop-output.txt>
Matches each laptop block (t_start/t_end, epoch seconds) to the node's counter log and prints per-block
goodput next to the node's retransmission rate, then per-variant medians."""
import json, re, statistics, sys

rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
def at(t, before):
    # Bracket by the node's own snapshot time (ts_ms), not by when the logger read the file.
    cands = [r for r in rows if (r["ts_ms"] / 1000 <= t if before else r["ts_ms"] / 1000 >= t)]
    return (max if before else min)(cands, key=lambda r: r["ts_ms"]) if cands else None

blocks = []
for line in open(sys.argv[2]):
    m = re.search(r"block=(\d+) slot=(\d+) pacing=(\w+) t_start=([\d.]+) t_end=([\d.]+) single=([\d.]+),([\d.]+) Mbit/s par8=([\d.]+)", line)
    if not m:
        continue
    b, slot, v, t0, t1, s1, s2, p8 = m.groups()
    a, z = at(float(t0), True), at(float(t1), False)
    if not a or not z:
        print(f"block {b}: no counters around it"); continue
    d = {k: z[k] - a[k] for k in ("data_segs", "retx_segs", "fast_retx", "rto", "paced_deferrals")}
    ok = a["pacing"] == z["pacing"] == (v == "on")
    pct = 100.0 * d["retx_segs"] / d["data_segs"] if d["data_segs"] else 0.0
    blocks.append((v, float(s1), float(s2), float(p8), pct, d, ok))
    print(f"block {b} pacing={v:3} node_pacing_ok={ok} single={s1},{s2} par8={p8} Mbit/s "
          f"data_segs={d['data_segs']} retx={d['retx_segs']} ({pct:.2f}%) fast_retx={d['fast_retx']} "
          f"rto={d['rto']} deferrals={d['paced_deferrals']}")

for v in ("off", "on"):
    bs = [x for x in blocks if x[0] == v]
    if not bs:
        continue
    singles = [s for x in bs for s in (x[1], x[2])]
    print(f"\n{v}: blocks={len(bs)} single median={statistics.median(singles):.0f} Mbit/s "
          f"(range {min(singles):.0f}-{max(singles):.0f}); par8 median={statistics.median([x[3] for x in bs]):.0f}; "
          f"retx% median={statistics.median([x[4] for x in bs]):.2f}; "
          f"rto total={sum(x[5]['rto'] for x in bs)}; fast_retx total={sum(x[5]['fast_retx'] for x in bs)}")
