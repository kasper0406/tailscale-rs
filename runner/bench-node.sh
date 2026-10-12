#!/bin/zsh
# Restart the node for synthetic benchmarking (redirect to the local sink) and point the peer
# configs at its new disco key. usage: runner/bench-node.sh <binary> [extra env assignments...]
set -eu
cd /Volumes/git/ts-exit
bin=$1; shift
S=state/rust-exit
node_disco() { runner/tsc debug netmap 2>/dev/null | python3 -c 'import json,sys; nm=json.load(sys.stdin); print([p["DiscoKey"] for p in nm["Peers"] if "rust-exit." in p.get("Name","")][0].split(":")[1])'; }
before=$(node_disco)
[ -f $S/exit_node.pid ] && kill $(cat $S/exit_node.pid) 2>/dev/null && sleep 3
python3 -I runner/detach.py $S/exit_node.log /usr/bin/env TS_RS_EXPERIMENT=this_is_unstable_software RUST_LOG=info \
  TS_BENCH_EXIT_REDIRECT=198.18.0.1:9000=127.0.0.1:9100 TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1 \
  TS_DEBUG_DONT_FRAGMENT=1 TS_DEBUG_MAGICSOCK_BATCHED_IO=1 "$@" $PWD/$bin --state-dir $PWD/$S \
  --hostname rust-exit --port 41642 --interactive --status-interval 10 > $S/exit_node.pid
for i in $(seq 1 30); do
  disco=$(node_disco)
  [ -n "$disco" ] && [ "$disco" != "$before" ] && break; sleep 2
done
echo $disco > scratch/last-disco
for f in bench/results/stage0/peers.json bench/results/stage0/peers1.json; do
  python3 -c 'import json,sys; p=json.load(open(sys.argv[1])); [e.update(node_disco_key=sys.argv[2]) for e in p]; json.dump(p,open(sys.argv[1],"w"),indent=1)' $f $disco
done
sed 's/\x1b\[[0-9;]*m//g' $S/exit_node.log | grep -E "running v4|BENCHMARK|pipeline" | tail -3
echo "node pid $(cat $S/exit_node.pid) disco $disco"
