#!/bin/zsh
# Started by launchd (dev.kasper.rust-exit) as the owning user, at boot and before login.
# Runs the exit node from this directory and writes its log through rotate.pl.
# Installed by install.sh into ~/Library/Application Support/rust-exit/.
set -u
dir=${0:A:h}
logdir=${dir:h:h}/Logs/rust-exit
mkdir -p $logdir

export TS_RS_EXPERIMENT=this_is_unstable_software
export RUST_LOG=${RUST_LOG:-info}
export TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1 TS_DEBUG_DONT_FRAGMENT=1 TS_DEBUG_MAGICSOCK_BATCHED_IO=1

# --interactive only matters if state/keys.json is missing: the node then logs a login URL instead of exiting.
$dir/bin/exit_node --state-dir $dir/state --hostname rust-exit --port 41642 --interactive \
  --status-interval 300 --tcp-buffer-size 1048576 \
  > >(/usr/bin/perl $dir/rotate.pl $logdir/exit_node.log) 2>&1 &
pid=$!
# launchd stops the job with SIGTERM to this script; pass it on so the node shuts down cleanly.
trap 'kill -TERM $pid 2>/dev/null' TERM INT HUP
wait $pid; rc=$?
while kill -0 $pid 2>/dev/null; do wait $pid; rc=$?; done  # a trapped signal interrupts wait
exit $rc
