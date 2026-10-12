#!/bin/zsh
# One A/B row: single flow (3 x 1 GB each way), 8 parallel 500 MB flows + echo probe.
# usage: RUNNER_ARGS="--worker-threads 4" runner/ab-matrix.sh <label> <binary> [ENV=val ...]
set -u
R=/Volumes/git/ts-exit; label=$1 bin=$2; shift 2
$R/runner/tunnel-bench.sh $label $bin 3 1000 "$@" | tee -a $R/bench/results/tunnel-ab.csv
P=$(cat $R/state/rust-exit/exit_node.pid)
cpu() { ps -o time= -p $P | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else print $1*3600+$2*60+$3+$4/100 }'; }
c0=$(cpu); t0=$(python3 -c 'import time;print(time.time())')
for i in $(seq 8); do python3 -I $R/runner/sinkclient.py 127.0.0.1:1055 100.112.127.25:9000 d 500000000 >/dev/null & done
sleep 1; lat=$(python3 -I $R/runner/rttprobe.py 127.0.0.1:1055 100.112.127.25:9000 150); wait
t1=$(python3 -c 'import time;print(time.time())'); c1=$(cpu)
python3 -c "w=$t1-$t0; print(f'$label,par8,1,{8*500e6*8/w/1e6:.0f},{($c1-$c0):.2f},{($c1-$c0)/4:.2f}')" | tee -a $R/bench/results/tunnel-ab.csv
echo "$label echo-under-8-flows: $lat" | tee -a $R/bench/results/latency.txt
