#!/bin/zsh
# Stop and remove the daemon. Leaves ~/Library/Application Support/rust-exit (keys) and the logs in place.
set -u
label=dev.kasper.rust-exit
sudo launchctl bootout system/$label 2>/dev/null || true
sudo rm -f /Library/LaunchDaemons/$label.plist
echo "removed $label"
