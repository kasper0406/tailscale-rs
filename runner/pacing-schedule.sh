#!/bin/zsh
# usage: runner/pacing-schedule.sh <minutes> <log.jsonl>
# For the real-world pacing A/B without SSH: switches the node's pacing on a fixed UTC schedule (off in even
# 120 s slots, on in odd ones: on = (epoch / 120) % 2 == 1) and logs the node's TCP counters every second.
# Leaves pacing off at the end, whatever happens.
set -u
mins=${1:?minutes}; log=${2:?log}
app="$HOME/Library/Application Support/rust-exit"
trap 'echo off > "$app/pacing"' EXIT INT TERM
end=$(( $(date +%s) + mins * 60 ))
while (( $(date +%s) < end )); do
  now=$(date +%s)
  (( (now / 120) % 2 == 1 )) && want=on || want=off
  [[ $(cat "$app/pacing" 2>/dev/null) == $want ]] || echo $want > "$app/pacing"
  python3 -c 'import json,sys,time; d=json.load(open(sys.argv[1])); d["log_ts"]=time.time(); print(json.dumps(d))' "$app/tcp-stats.json" >> $log 2>/dev/null
  sleep 1
done
