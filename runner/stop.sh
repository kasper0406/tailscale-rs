#!/bin/zsh
# Stop the Rust exit node (SIGTERM, then SIGKILL after 10 s). This is the whole rollback for the
# userspace node: it changed nothing on the host. Peers using it as an exit node lose internet
# until they pick another exit node or none.
set -u
source ${0:A:h}/env.sh
[[ -f $TS_EXIT_PIDFILE ]] || { echo "not running"; exit 0; }
pid=$(cat $TS_EXIT_PIDFILE)
if kill -0 $pid 2>/dev/null; then
  kill -TERM $pid
  for i in {1..20}; do kill -0 $pid 2>/dev/null || break; sleep 0.5; done
  kill -0 $pid 2>/dev/null && { echo "SIGKILL"; kill -KILL $pid; }
fi
rm -f $TS_EXIT_PIDFILE
echo stopped
