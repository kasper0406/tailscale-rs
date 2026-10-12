#!/bin/zsh
# Scripts only: user executes; restarts the specified node using the existing authorized launcher.
# ROOT=/Volumes/git/ts-exit MODES=0,1 RUNS=8 RUNNER_ARGS='--worker-threads 4' runner/ab-matrix2.sh label binary [ENV=value ...]
set -euo pipefail
R=${ROOT:-/Volumes/git/ts-exit}
label=$1 bin=${2:A}; shift 2
modes=(${(s:,:)${MODES:-0,1}})
for mode in $modes; do
  [[ $mode == 0 || $mode == 1 ]] || { print -u2 "Only modes 0/1 exist; stage split is parked and sharding is not implemented"; exit 2; }
done
flags=$(python3 - "$@" <<'PYFLAGS'
import os, sys, json, shlex
names=("TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS", "TS_DEBUG_DONT_FRAGMENT", "TS_DEBUG_MAGICSOCK_BATCHED_IO")
values={name:os.environ.get(name,"unset") for name in names}
for assignment in sys.argv[1:]:
    name,sep,value=assignment.partition("=")
    if sep and name in values: values[name]=value
options=shlex.split(os.environ.get("RUNNER_ARGS","--worker-threads 4"))
for flag in ("--worker-threads","--tcp-buffer-size"):
    if flag in options:
        index=options.index(flag)
        values[flag]=options[index+1] if index+1<len(options) else "missing"
print(json.dumps(values,sort_keys=True))
PYFLAGS
)
runs=${RUNS:-8}
bytes=${BYTES:-500000000}
output=${OUTPUT:-$R/bench/results/matrix2-$label.jsonl}
mkdir -p ${output:h}
# Reverse every second pass, avoiding monotonic temperature/order bias.
for run in $(seq $runs); do
  order=($modes)
  (( run % 2 == 0 )) && order=(${(Oa)modes})
  for mode in $order; do
    # Startup/warmup transfers have their own window and are excluded from matrix rows.
    $R/runner/tunnel-bench.sh "$label-warm-$mode" "$bin" 1 1 "$@" \
      TS_EXPERIMENT_PIPELINE_THREAD=$mode TS_DEBUG_PIPELINE_METRICS=1 >/dev/null
    pid=$(cat $R/state/rust-exit/exit_node.pid)
    for flows in 1 8; do
      for dir in d u; do
        python3 -I ${0:A:h}/matrix2.py --destination ${DESTINATION:-100.112.127.25:9000} \
          --proxy ${PROXY:-127.0.0.1:1055} --direction $dir --flows $flows --bytes $bytes \
          --node-pid $pid --binary "$bin" --label "$label-mode$mode-run$run" \
          --flags "pipeline=$mode; metrics=1; $flags" >> "$output"
      done
    done
    # Cumulative metrics: derive deltas per status interval; don't sum cumulative lines.
    sed -n '/pipeline metrics (cumulative)/p' $R/state/rust-exit/exit_node.log \
      > "${output:r}-mode$mode-run$run.metrics.txt"
  done
done
