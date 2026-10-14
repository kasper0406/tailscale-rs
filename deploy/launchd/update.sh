#!/bin/zsh
# Swap in a new binary for the running daemon without sudo: install it, then stop the node;
# launchd restarts it (KeepAlive) on the new binary within ~10 s.
# usage: deploy/launchd/update.sh [binary]   (default: the fork's target/exitnode/examples/exit_node)
set -eu
ws=${0:A:h:h:h}
bin=${1:-$ws/repos/geiserx-tailscale-rs/target/exitnode/examples/exit_node}
app="$HOME/Library/Application Support/rust-exit"
[[ -x $bin ]] || { echo "no binary at $bin"; exit 1; }
install -m 755 "$bin" "$app/bin/exit_node.new" && mv -f "$app/bin/exit_node.new" "$app/bin/exit_node"
pkill -TERM -f "^$app/bin/exit_node " || echo "node was not running"
sleep 15
pgrep -fl "^$app/bin/exit_node " || { echo "node did not come back; see ~/Library/Logs/rust-exit/"; exit 1; }
