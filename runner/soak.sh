#!/bin/zsh
# Light soak monitor for the live node: every 30 min, egress IP + DNS/HTTPS through the exit node
# (Go test client via SOCKS) and the node's RSS/CPU. Appends one line per check to state/soak.log.
cd /Volumes/git/ts-exit
while true; do
  pid=$(cat state/rust-exit/exit_node.pid 2>/dev/null)
  stats=$(ps -o rss=,time= -p $pid 2>/dev/null | awk '{printf "rss_mb=%d cpu=%s", $1/1024, $2}')
  ip=$(perl -e 'alarm 20; exec @ARGV' curl -s --socks5-hostname 127.0.0.1:1055 https://ifconfig.me 2>/dev/null)
  code=$(perl -e 'alarm 20; exec @ARGV' curl -s -o /dev/null -w '%{http_code}' --socks5-hostname 127.0.0.1:1055 https://example.com 2>/dev/null)
  echo "$(date -u +%FT%TZ) pid=$pid ${stats:-node=DOWN} egress=${ip:-FAIL} https=${code:-FAIL}" >> state/soak.log
  sleep 1800
done
