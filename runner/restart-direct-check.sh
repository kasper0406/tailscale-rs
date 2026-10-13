#!/bin/zsh
# Restart the configured live node, then measure its own path to one exact peer name.
# usage: RESTART_DIRECT_BIN=/absolute/path/to/exit_node restart-direct-check.sh PEER [-- NODE_ARGS...]
# Run only when a live restart is intended. No build or client/tailnet commands are issued.
set -eu
runner=${0:A:h}
source "$runner/env.sh"
if (( $# == 0 )); then
  echo "usage: $0 PEER [-- NODE_ARGS...]" >&2
  exit 2
fi
peer=$1
shift
if (( $# )); then
  [[ $1 == -- ]] || { echo 'expected -- before node arguments' >&2; exit 2; }
  shift
fi
node_bin=${RESTART_DIRECT_BIN:-$TS_EXIT_BIN}
[[ -x $node_bin ]] || { echo "not executable: $node_bin" >&2; exit 2; }
[[ -f $TS_EXIT_PIDFILE ]] || { echo 'node is not running' >&2; exit 2; }
kill -0 "$(cat "$TS_EXIT_PIDFILE")" 2>/dev/null || { echo 'node is not running' >&2; exit 2; }

"$runner/stop.sh"
# Exclude every pre-restart status line in the append-only log.
offset=$(python3 -I -c 'import os,sys; print(os.path.getsize(sys.argv[1]) if os.path.exists(sys.argv[1]) else 0)' "$TS_EXIT_LOG")
args=(--state-dir "$TS_EXIT_STATE" --hostname "$TS_EXIT_HOSTNAME" --port "$TS_EXIT_PORT" --interactive)
[[ -n $TS_EXIT_TAG ]] && args+=(--tag "$TS_EXIT_TAG")
started=$(python3 -I -c 'import time; print(time.monotonic())')
pid=$(python3 -I "$runner/detach.py" "$TS_EXIT_LOG" /usr/bin/env \
  TS_RS_EXPERIMENT=this_is_unstable_software RUST_LOG="${RUST_LOG:-info}" \
  "$node_bin" "${args[@]}" --status-interval 1 "$@")
print -r -- "$pid" > "$TS_EXIT_PIDFILE"

python3 -I - "$TS_EXIT_LOG" "$peer" "$offset" "$started" "$pid" <<'PY'
import os
import re
import sys
import time

log, peer, offset, started, pid = sys.argv[1:]
offset, started, pid = int(offset), float(started), int(pid)
ansi = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]')
pending = ''
last_path = 'no status seen'
while time.monotonic() - started < 120:
    try:
        with open(log, encoding='utf-8', errors='replace') as stream:
            if os.fstat(stream.fileno()).st_size < offset:
                offset, pending = 0, ''
            stream.seek(offset)
            pending += stream.read()
            offset = stream.tell()
    except FileNotFoundError:
        pass
    lines = pending.split('\n')
    pending = lines.pop()
    for line in lines:
        line = ansi.sub('', line)
        name = re.search(r'\bpeer=(.*?)\s+ip=', line)
        if name is None or name[1].strip('"') != peer:
            continue
        path = re.search(r'\bpath=(direct|derp|idle)\b', line)
        if path is None:
            continue
        last_path = path[1]
        if last_path == 'direct':
            elapsed = time.monotonic() - started
            print(f'PASS peer={peer} path=direct after {elapsed:.1f} seconds (status polled every 1s)')
            sys.exit(0)
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        print(f'FAIL peer={peer}: node exited before path=direct', file=sys.stderr)
        sys.exit(1)
    time.sleep(1)
print(f'TIMEOUT peer={peer}: no path=direct within 120 seconds; last path={last_path}', file=sys.stderr)
sys.exit(1)
PY
