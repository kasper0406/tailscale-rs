#!/bin/zsh
# A/B harness: restart the Rust exit node with <binary> (+ extra env/args), then push N downloads
# through the local Go test client (SOCKS5 127.0.0.1:1055) and report throughput and the node's CPU.
# usage: runner/perf.sh <label> <binary> [N=3] [MB=200] -- [extra env assignments...] [-- runner args...]
set -u
label=$1 bin=$2 n=${3:-3} mb=${4:-200}
shift $(( $# < 4 ? $# : 4 ))
envs=() rargs=()
if [[ ${1:-} == -- ]]; then shift; while (( $# )) && [[ $1 != -- ]]; do envs+=$1; shift; done; fi
if [[ ${1:-} == -- ]]; then shift; rargs=("$@"); fi
R=/Volumes/git/ts-exit; S=$R/state/rust-exit
[[ -f $S/exit_node.pid ]] && kill $(cat $S/exit_node.pid) 2>/dev/null; sleep 3
python3 -I $R/runner/detach.py $S/exit_node.log /usr/bin/env TS_RS_EXPERIMENT=this_is_unstable_software RUST_LOG=info $envs \
  $bin --state-dir $S --hostname rust-exit --port 41642 --interactive --status-interval 10 $rargs > $S/exit_node.pid
pid=$(cat $S/exit_node.pid)
until sed 's/\x1b\[[0-9;]*m//g' $S/exit_node.log | grep -q "running v4"; do sleep 1; done; sleep 6
$R/runner/tsc set --exit-node=rust-exit
X=(--socks5-hostname 127.0.0.1:1055); U="https://proof.ovh.net/files/1Gb.dat"
curl -s -m 30 $X -o /dev/null https://example.com   # warm the path and ports
cpu() { ps -o time= -p $pid | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else print $1*3600+$2*60+$3+$4/100 }'; }
for i in $(seq $n); do
  c0=$(cpu); t0=$(python3 -c 'import time;print(time.time())')
  bps=$(curl -s -m 120 $X -r 0-$((mb*1000000-1)) -o /dev/null -w '%{speed_download}' "$U")
  t1=$(python3 -c 'import time;print(time.time())'); c1=$(cpu)
  python3 -c "
bps=float('$bps'); cpu=$c1-$c0; wall=$t1-$t0; gb=$mb/1000
print(f'$label,$i,{bps*8/1e6:.0f},{cpu:.2f},{cpu/gb:.2f},{100*cpu/wall:.0f}')"
done
