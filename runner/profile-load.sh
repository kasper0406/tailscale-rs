#!/bin/zsh
# Profile the node under a sustained synthetic load. usage: runner/profile-load.sh <down|up> <flows/peer> <bytes/flow> <out.txt>
set -u
cd /Volumes/git/ts-exit
dir=$1; flows=$2; bytes=$3; out=$4
NP=$(cat state/rust-exit/exit_node.pid); DK=$(cat scratch/last-disco)
src=(x 100.127.38.40 100.108.232.5)
holders=(); peers=()
for i in 1 2; do
  bin/synthetic_peer hold --key-file state/bench-peers/$i/node.key --disco-key-file state/bench-peers/$i/disco.key \
    --identity-file state/bench-peers/$i/identity.json --advertise 192.168.50.76:4170$i --hostname synthetic-load-$i >/dev/null 2>&1 &
  holders+=($!)
done
trap 'kill $holders $peers 2>/dev/null' EXIT
sleep 15
for i in 1 2; do
  bin/synthetic_peer --workers 2 peer --key-file state/bench-peers/$i/node.key --disco-key-file state/bench-peers/$i/disco.key \
    --node-key 23f7e5019ea1f115651f6314e489f14999550f3f94de96a1e5ef897024b07f19 --node-disco-key $DK --bind 0.0.0.0:4170$i \
    --await-node-ping --endpoint 192.168.50.76:41642 --source ${src[$((i+1))]} --destination 198.18.0.1:9000 \
    --flows $flows --bytes $bytes --direction $dir --timeout 150 > $out.peer$i.json 2>&1 &
  peers+=($!)
done
for i in $(seq 1 40); do c=$(ps -o %cpu= -p $NP | tr -d ' '); [ ${c%.*} -gt 100 ] && break; sleep 1; done
sleep 3
sample $NP 6 -mayDie -file $out >/dev/null 2>&1
wait $peers
peers=()
python3 -c 'import json,sys
for f in sys.argv[1:]:
  d=json.loads(open(f).read().strip().splitlines()[-1]); print(f.split(".")[-2], round(d["mbit_s"]), "Mbit/s")' $out.peer1.json $out.peer2.json
