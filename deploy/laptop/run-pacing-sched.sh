#!/bin/zsh
# usage: run-pacing-sched.sh <label> [blocks]
# Real-world pacing A/B through the exit node kmini, without SSH. The Mac switches pacing on a fixed UTC
# schedule (runner/pacing-schedule.sh: on when (epoch / 120) % 2 == 1) and logs its TCP counters. This script
# runs one measurement block in each 120 s slot, starting 8 s after the switch, and prints UTC timestamps so the
# counters can be matched afterwards. Default 6 blocks (3 off, 3 on), about 12 minutes.
set -u
ts=/Applications/Tailscale.app/Contents/MacOS/Tailscale
label=$1; blocks=${2:-6}
out=~/ts-exit-test/$label-$(date +%H%M%S).txt
apple=https://mensura.cdn-apple.com/api/v1/gm/large
tmp=$(mktemp -d)
prev=$($ts exit-node list 2>/dev/null | awk '/selected/ {print $2}' | sed 's/\..*//' | head -1)
restore() { $ts set --exit-node=${prev:-} >/dev/null 2>&1; }
trap restore EXIT

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
par() {  # $1 flows: aggregate Mbit/s of bytes received in 10 s
  local f=$tmp/p; : > $f
  local t0=$(now)
  for i in $(seq $1); do curl -s -m 10 -o /dev/null -w '%{size_download}\n' $apple >> $f & done; wait
  local t1=$(now)
  perl -e '($t0,$t1,$f)=@ARGV; open F,$f; while(<F>){$s+=$_} printf "%.0f", $s*8/1e6/($t1-$t0)' $t0 $t1 $f
}

{
echo "== $label $(date -u +%FT%TZ) blocks=$blocks"
$ts set --exit-node=kmini; sleep 4
$ts status | grep ' kmini ' | sed -E 's/[0-9]{1,3}(\.[0-9]{1,3}){3}(:[0-9]+)?/<ip>/g'
for b in $(seq $blocks); do
  # Wait for the next slot boundary, then 8 s for the switch to take effect.
  e=$(date +%s); slot=$(( e / 120 + 1 )); sleep $(( slot * 120 + 8 - e ))
  (( slot % 2 == 1 )) && v=on || v=off
  # One throwaway connection, so the node's port-443 listener holds a socket with the new setting.
  curl -s -m 5 -o /dev/null -r 0-1023 $apple
  t_start=$(now)
  single1=$(par 1); single2=$(par 1); par8=$(par 8)
  t_end=$(now)
  echo "block=$b slot=$slot pacing=$v t_start=$t_start t_end=$t_end single=${single1},${single2} Mbit/s par8=${par8} Mbit/s"
done
} 2>&1 | tee $out
