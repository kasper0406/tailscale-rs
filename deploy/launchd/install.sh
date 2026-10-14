#!/bin/zsh
# Install (or reinstall) the exit node as a boot-time launchd daemon running as the current user.
# usage: deploy/launchd/install.sh [binary]   (default: the fork's target/exitnode/examples/exit_node)
# Run it as yourself, not with sudo; it asks for sudo only to install and load the plist.
set -eu
src=${0:A:h}
ws=${src:h:h}
bin=${1:-$ws/repos/geiserx-tailscale-rs/target/exitnode/examples/exit_node}
app="$HOME/Library/Application Support/rust-exit"
logs="$HOME/Library/Logs/rust-exit"
label=dev.kasper.rust-exit
plist=/Library/LaunchDaemons/$label.plist
old_state=$ws/state/rust-exit

[[ $EUID -ne 0 ]] || { echo "run as yourself, not root"; exit 1; }
[[ -x $bin ]] || { echo "no binary at $bin: build it first (runner/build.sh)"; exit 1; }

mkdir -p "$app/bin" "$app/state" "$logs"
chmod 700 "$app" "$app/state" "$logs"
install -m 755 "$bin" "$app/bin/exit_node.new" && mv -f "$app/bin/exit_node.new" "$app/bin/exit_node"
install -m 755 "$src/run.sh" "$app/run.sh"
install -m 644 "$src/rotate.pl" "$app/rotate.pl"

# Carry the node's identity over from the workspace state dir. Only one process may ever use these
# keys, so stop the manually started node first.
if [[ ! -f "$app/state/keys.json" ]]; then
  [[ -f $old_state/keys.json ]] || { echo "no keys.json in $old_state"; exit 1; }
  if [[ -f $old_state/exit_node.pid ]] && kill -0 "$(cat $old_state/exit_node.pid)" 2>/dev/null; then
    pid=$(cat $old_state/exit_node.pid)
    echo "stopping the manually started node (pid $pid)"
    kill -TERM $pid
    for i in {1..20}; do kill -0 $pid 2>/dev/null || break; sleep 0.5; done
    kill -0 $pid 2>/dev/null && kill -KILL $pid
    rm -f $old_state/exit_node.pid
  fi
  install -m 600 "$old_state/keys.json" "$app/state/keys.json"
  # Retire the old copy so runner/start.sh can't start a second node with the same identity.
  mv "$old_state/keys.json" "$old_state/keys.json.migrated-to-launchd"
  echo "moved node identity to $app/state"
fi

tmp=$(mktemp)
sed -e "s|@USER@|$USER|g" -e "s|@HOME@|$HOME|g" -e "s|@APP@|$app|g" "$src/$label.plist" > $tmp
plutil -lint $tmp >/dev/null
echo "installing $plist (needs sudo)"
sudo launchctl bootout system/$label 2>/dev/null || true
sudo install -m 644 -o root -g wheel $tmp $plist
rm -f $tmp
sudo launchctl enable system/$label
sudo launchctl bootstrap system $plist
sleep 8
launchctl print system/$label 2>/dev/null | grep -E "^\s*(state|pid|last exit code)" || true
tail -n 5 "$logs/exit_node.log" 2>/dev/null | sed -E 's/\x1b\[[0-9;]*m//g' | cut -c1-160
