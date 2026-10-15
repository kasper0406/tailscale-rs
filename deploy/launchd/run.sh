#!/bin/zsh
# Started by launchd (dev.kasper.rust-exit) as the owning user, at boot and before login.
# Runs the exit node from this directory and writes its log through rotate.pl.
# Installed by install.sh into ~/Library/Application Support/rust-exit/.
set -u
dir=${0:A:h}
logdir=${dir:h:h}/Logs/rust-exit
mkdir -p $logdir
# Wrapper errors go to launchd.log (StandardOutPath), which nothing rotates: keep it small.
[[ -f $logdir/launchd.log && $(stat -f %z $logdir/launchd.log) -gt 1048576 ]] && : > $logdir/launchd.log

export TS_RS_EXPERIMENT=this_is_unstable_software
export RUST_LOG=${RUST_LOG:-info}
export TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1 TS_DEBUG_DONT_FRAGMENT=1 TS_DEBUG_MAGICSOCK_BATCHED_IO=1
# Opt-in upstream. Off until A/B-tested on this build one at a time (design review 28):
# export TS_EXPERIMENT_RX_BATCH=1 TS_EXPERIMENT_SPLICE_COALESCE=1 TS_EXPERIMENT_TARGETED_REPLAY=1

# --interactive only matters if state/keys.json is missing: the node then logs a login URL instead of exiting.
# --serve-tcp 22=127.0.0.1:22: `ssh kmini` from the tailnet reaches this Mac's sshd.
# 1 MiB TCP window: needs the patched smoltcp in this build (ts-exit/tcp-0.14: NewReno, SACK, pacing,
# 200 ms RTO floor; docs/TCP-WORKAROUND-30.md). With stock smoltcp use the 256 KiB default: larger
# windows stall on lossy links (docs/DECISIONS.md F31, F34).
$dir/bin/exit_node --state-dir $dir/state --hostname kmini --port 41642 --interactive \
  --status-interval 300 --tcp-buffer-size 1048576 --serve-tcp 22=127.0.0.1:22 \
  > >(/usr/bin/perl $dir/rotate.pl $logdir/exit_node.log) 2>&1 &
pid=$!
# launchd stops the job with SIGTERM to this script; pass it on so the node shuts down cleanly.
trap 'kill -TERM $pid 2>/dev/null' TERM INT HUP
wait $pid; rc=$?
while kill -0 $pid 2>/dev/null; do wait $pid; rc=$?; done  # a trapped signal interrupts wait
exit $rc
