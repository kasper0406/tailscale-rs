#!/bin/zsh
# 8 parallel 500 MB downloads + echo probe against the running node; prints aggregate, CPU/GB, latency.
R=/Volumes/git/ts-exit; label=$1; P=$(cat $R/state/rust-exit/exit_node.pid)
cpu() { ps -o time= -p $P | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else print $1*3600+$2*60+$3+$4/100 }'; }
c0=$(cpu); t0=$(python3 -c 'import time;print(time.time())')
for i in $(seq 8); do python3 -I $R/runner/sinkclient.py 127.0.0.1:1055 100.112.127.25:9000 d 500000000 >/dev/null & done
sleep 1; lat=$(python3 -I $R/runner/rttprobe.py 127.0.0.1:1055 100.112.127.25:9000 150); wait
t1=$(python3 -c 'import time;print(time.time())'); c1=$(cpu)
python3 -c "w=$t1-$t0; print(f'$label,par8,{8*500e6*8/w/1e6:.0f} Mbit/s,{($c1-$c0)/4:.2f} s/GB', end=' ')"; echo "| $lat"
