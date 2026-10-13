#!/bin/zsh
# Stage-0 baseline through the live node: synthetic peers -> node -> TS_BENCH_EXIT_REDIRECT -> local sink.
# usage: runner/stage0-matrix.sh <label> <outdir> [configs...]   (node + sink must already be running)
# configs default to: p1f1 p2f1 p2f8 p2f8rtt p2f64
set -u
cd /Volumes/git/ts-exit
label=$1; out=$2; shift 2; mkdir -p $out
configs=("$@"); (( $#configs )) || configs=(p1f1 p2f1 p2f8 p2f8rtt p2f64)
pid=$(cat state/rust-exit/exit_node.pid)
node_bin=$(ps -o command= -p $pid | awk '{print $1}')
P1=bench/results/stage0/peers1.json; P2=bench/results/stage0/peers.json
# Fresh endpoint ports per matrix: the node only probes a peer endpoint when it appears/changes in the netmap,
# and control remembers a peer's last endpoint across sessions, so reusing a port leaves the node on DERP.
base=$(( 41710 + (RANDOM % 140) * 2 ))
for f in $P1 $P2; do
  python3 -c 'import json,sys; b=int(sys.argv[2]); p=json.load(open(sys.argv[1])); [e.update(bind=f"0.0.0.0:{b+i}", advertise=f"192.168.50.76:{b+i}") for i,e in enumerate(p)]; json.dump(p,open(sys.argv[1],"w"),indent=1)' $f $base
done
# One control session per peer for the whole matrix, settled before any run (see synthetic_peer hold).
holders=()
for i in 1 2; do
  bin/synthetic_peer hold --key-file state/bench-peers/$i/node.key --disco-key-file state/bench-peers/$i/disco.key \
    --identity-file state/bench-peers/$i/identity.json --advertise 192.168.50.76:$((base+i-1)) --hostname synthetic-load-$i \
    > /dev/null 2>> $out/errors.log &
  holders+=($!)
done
trap 'kill $holders 2>/dev/null' EXIT
sleep 15
run() { # name peersfile flows bytes runs
  perl -e 'alarm 1200; exec @ARGV' python3 runner/synthetic-matrix.py --binary bin/synthetic_peer --peers $2 \
    --node-pid $pid --node-binary $node_bin --label "$label/$1" --flows-per-peer $3 --bytes $4 \
    --workers 2 --runs $5 --timeout 120 --external-hold >> $out/$1.jsonl 2>> $out/errors.log
}
[ -f $out/rtt-idle.txt ] || python3 runner/rttprobe.py 127.0.0.1:1055 198.18.0.1:9000 300 > $out/rtt-idle.txt 2>&1
for c in $configs; do case $c in
  p1f1) run p1f1 $P1 1 2000000000 3 ;;
  p2f1) run p2f1 $P2 1 2000000000 3 ;;
  p2f8) run p2f8 $P2 8 250000000 3 ;;
  p2f8rtt) ( python3 runner/rttprobe.py 127.0.0.1:1055 198.18.0.1:9000 1500 > $out/rtt-load-p2f8.txt 2>&1 & )
           run p2f8rtt $P2 8 250000000 3 ;;
  p2f64) run p2f64 $P2 64 30000000 3 ;;
esac; done
echo done > $out/DONE
