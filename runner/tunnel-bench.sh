#!/bin/zsh
# Tunnel-only A/B: restart the Rust node (<binary> + env) with the benchmark sink on :9000, then move
# <MB> through the local Go client (SOCKS5 :1055) to the node's own tailnet address, each direction,
# <N> times. Prints label,dir,run,mbit_s,node_cpu_s,node_cpu_s_per_gb.
# usage: runner/tunnel-bench.sh <label> <binary> [N=3] [MB=1000] [ENV=val ...]
set -u
label=$1 bin=$2 n=${3:-3} mb=${4:-1000}; shift $(( $# < 4 ? $# : 4 ))
R=/Volumes/git/ts-exit; S=$R/state/rust-exit; IP=100.112.127.25
[[ -f $S/exit_node.pid ]] && kill $(cat $S/exit_node.pid) 2>/dev/null; sleep 3
python3 -I $R/runner/detach.py $S/exit_node.log /usr/bin/env TS_RS_EXPERIMENT=this_is_unstable_software RUST_LOG=info "$@" \
  $bin --state-dir $S --hostname rust-exit --port 41642 --interactive --status-interval 10 --sink-port 9000 ${=RUNNER_ARGS:-} > $S/exit_node.pid
pid=$(cat $S/exit_node.pid)
until sed 's/\x1b\[[0-9;]*m//g' $S/exit_node.log | grep -q "benchmark sink listening"; do sleep 1; done; sleep 6
$R/runner/tsc ping -c 2 rust-exit >/dev/null 2>&1
cpu() { ps -o time= -p $pid | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else print $1*3600+$2*60+$3+$4/100 }'; }
now() { python3 -c 'import time;print(time.time())'; }
bytes=$((mb*1000000))
for i in $(seq $n); do
  for dir in d u; do
    c0=$(cpu)
    read got secs <<< $(python3 -I $R/runner/sinkclient.py 127.0.0.1:1055 $IP:9000 $dir $bytes)
    c1=$(cpu)
    python3 -c "
wall=$secs; cpu=$c1-$c0; gb=$got/1e9
print(f'$label,{\"down\" if \"$dir\"==\"d\" else \"up\"},$i,{$got*8/wall/1e6:.0f},{cpu:.2f},{cpu/gb if gb else 0:.2f}')"
  done
done
