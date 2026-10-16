#!/bin/zsh
# Swap in a new release (binary + run.sh + rotate.pl from this directory) for the running daemon, without
# sudo, and roll back automatically if it isn't healthy within 90 s.
# usage: deploy/launchd/update.sh <binary>      (RUN_SH=<wrapper> to install a different run.sh)
# Health = the node logs "running", an exit-node HTTPS fetch works through the local Go test client, and
# the node's tailnet SSH port answers (when run.sh serves it). Run it locally, not over `ssh kmini`: the
# restart drops that session.
set -u
src=${0:A:h}
ws=${src:h:h}
bin=${1:?usage: update.sh <binary>}
run=${RUN_SH:-$src/run.sh}
[[ -f $run ]] || { echo "no wrapper at $run"; exit 1; }
app="$HOME/Library/Application Support/rust-exit"
log="$HOME/Library/Logs/rust-exit/exit_node.log"
node_ip=100.112.127.25
socks=127.0.0.1:1055
[[ -x $bin ]] || { echo "no binary at $bin"; exit 1; }
"$bin" --help 2>/dev/null | grep -q -- --serve-tcp || ! grep -q -- --serve-tcp "$run" ||
  { echo "$bin lacks --serve-tcp, which run.sh uses; refusing"; exit 1; }

restart() {  # stop the node; launchd (KeepAlive) starts it again from the installed files
  pkill -TERM -f "^$app/bin/exit_node " || true
}
healthy() {  # $1 = log size before the restart
  local deadline=$((SECONDS + 90))
  while (( SECONDS < deadline )); do
    sleep 5
    # The node's log lines carry ANSI colour codes between target and message.
    tail -c +$(($1 + 1)) "$log" 2>/dev/null | sed -E 's/\x1b\[[0-9;]*m//g' | grep -q 'exit_node: running' || continue
    curl -s -m 15 --socks5-hostname $socks -o /dev/null -w '%{http_code}' https://example.com | grep -q 200 || continue
    if grep -q -- --serve-tcp "$app/run.sh"; then
      echo | nc -X 5 -x $socks -w 5 $node_ip 22 2>/dev/null | grep -q '^SSH-' || continue
    fi
    return 0
  done
  return 1
}

# Keep the running release, then install the new one.
for f in bin/exit_node run.sh rotate.pl; do cp -p "$app/$f" "$app/$f.prev"; done
install -m 755 "$bin" "$app/bin/exit_node.new" && mv -f "$app/bin/exit_node.new" "$app/bin/exit_node"
install -m 755 "$run" "$app/run.sh.new" && mv -f "$app/run.sh.new" "$app/run.sh"
install -m 644 "$src/rotate.pl" "$app/rotate.pl.new" && mv -f "$app/rotate.pl.new" "$app/rotate.pl"

size=$(stat -f %z "$log" 2>/dev/null || echo 0)
restart
if healthy $size; then
  echo "updated: $(pgrep -fl "^$app/bin/exit_node " | cut -c1-120)"
  exit 0
fi
echo "new release unhealthy after 90 s; rolling back"
for f in bin/exit_node run.sh rotate.pl; do mv -f "$app/$f.prev" "$app/$f"; done
size=$(stat -f %z "$log" 2>/dev/null || echo 0)
restart
healthy $size && echo "rolled back; previous release healthy" || echo "ROLLBACK ALSO UNHEALTHY: check $log"
exit 1
