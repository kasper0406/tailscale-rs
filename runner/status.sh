#!/bin/zsh
# Show whether the node runs, its latest self/peer status lines (direct vs DERP) and resource use.
set -u
source ${0:A:h}/env.sh
if [[ -f $TS_EXIT_PIDFILE ]] && kill -0 $(cat $TS_EXIT_PIDFILE) 2>/dev/null; then
  pid=$(cat $TS_EXIT_PIDFILE)
  echo "running pid $pid"; ps -o pid,etime,%cpu,rss,command -p $pid | cut -c1-120
else
  echo "not running"
fi
[[ -f $TS_EXIT_LOG ]] && grep -E ' (self|peer|running)' $TS_EXIT_LOG | tail -${1:-12}
