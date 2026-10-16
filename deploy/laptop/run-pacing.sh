#!/bin/zsh
# usage: run-pacing.sh <label> [reps]
# Real-world pacing A/B through the exit node kmini. Alternates the node's pacing off/on (over `ssh kmini`),
# downloads from Apple's CDN through kmini, and reads the node's TCP counters before and after each block, so
# every block reports goodput and the node's retransmission rate. Output also goes to ~/ts-exit-test/<label>-<time>.txt.
# The node's counters are node-wide: other devices using kmini during the run add noise.
set -u
ts=/Applications/Tailscale.app/Contents/MacOS/Tailscale
label=$1; reps=${2:-3}
out=~/ts-exit-test/$label-$(date +%H%M%S).txt
apple=https://mensura.cdn-apple.com/api/v1/gm/large
ctl='Library/Application Support/rust-exit/pacing'
stats='Library/Application Support/rust-exit/tcp-stats.json'
tmp=$(mktemp -d)

# Restore whatever exit node was selected before (by name), or none.
prev=$($ts exit-node list 2>/dev/null | awk '/selected/ {print $2}' | sed 's/\..*//' | head -1)
restore() { $ts set --exit-node=${prev:-} >/dev/null 2>&1; }
trap restore EXIT

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
node_stats() { ssh -o BatchMode=yes -o ConnectTimeout=10 kmini "cat \"\$HOME/$stats\""; }
set_pacing() {  # $1 = on|off; waits until the node reports it
  ssh -o BatchMode=yes -o ConnectTimeout=10 kmini "echo $1 > \"\$HOME/$ctl\"" || return 1
  local want=$([[ $1 == on ]] && echo true || echo false)
  for i in {1..10}; do
    sleep 1
    if node_stats | grep -q "\"pacing\": *$want"; then
      # The node's listener for port 443 holds a socket created before the switch; one throwaway
      # connection replaces it, so every measured connection uses the new setting.
      curl -s -m 5 -o /dev/null -r 0-1023 $apple
      return 0
    fi
  done
  echo "node did not switch pacing to $1"; return 1
}
par() {  # $1 flows: aggregate Mbit/s of bytes received in 10 s
  local f=$tmp/p; : > $f
  local t0=$(now)
  for i in $(seq $1); do curl -s -m 10 -o /dev/null -w '%{size_download}\n' $apple >> $f & done; wait
  local t1=$(now)
  perl -e '($t0,$t1,$f)=@ARGV; open F,$f; while(<F>){$s+=$_} printf "%.0f", $s*8/1e6/($t1-$t0)' $t0 $t1 $f
}
delta() {  # $1 before json, $2 after json -> "data_segs=.. retx=.. retx%=.. fast_retx=.. rto=.. deferrals=.."
  python3 -c '
import json, sys
a, b = json.loads(sys.argv[1]), json.loads(sys.argv[2])
keys = ["data_segs", "retx_segs", "fast_retx", "rto", "paced_deferrals"]
d = [b[k] - a[k] for k in keys]
pct = 100.0 * d[1] / d[0] if d[0] else 0.0
print("data_segs=%d retx=%d retx%%=%.2f fast_retx=%d rto=%d deferrals=%d" % (d[0], d[1], pct, d[2], d[3], d[4]))
' "$1" "$2"
}

{
echo "== $label $(date -u +%FT%TZ) reps=$reps"
$ts set --exit-node=kmini; sleep 4
$ts status | grep ' kmini ' | sed -E 's/[0-9]{1,3}(\.[0-9]{1,3}){3}(:[0-9]+)?/<ip>/g'
for r in $(seq $reps); do
  order=(off on); (( r % 2 == 0 )) && order=(on off)
  for v in $order; do
    set_pacing $v || exit 1
    s0=$(node_stats)
    single1=$(par 1); single2=$(par 1); par8=$(par 8)
    s1=$(node_stats)
    echo "rep=$r pacing=$v single=${single1},${single2} Mbit/s par8=${par8} Mbit/s $(delta "$s0" "$s1")"
  done
done
set_pacing off
} 2>&1 | tee $out
