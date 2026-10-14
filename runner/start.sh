#!/bin/zsh
# Start the Rust exit node in the background. Extra args are passed to the runner
# (e.g. --no-advertise for tunnel-only tests, --tcp-buffer-size 2097152).
# Userspace only: no root, no changes to host routes, DNS or pf.
set -eu
source ${0:A:h}/env.sh
if [[ -f /Library/LaunchDaemons/dev.kasper.rust-exit.plist ]]; then
  echo "the launchd daemon runs the node (deploy/launchd); use deploy/launchd/update.sh, or uninstall.sh first"
  exit 1
fi
mkdir -p $TS_EXIT_STATE && chmod 700 $TS_EXIT_STATE
if [[ -f $TS_EXIT_PIDFILE ]] && kill -0 $(cat $TS_EXIT_PIDFILE) 2>/dev/null; then
  echo "already running (pid $(cat $TS_EXIT_PIDFILE))"; exit 1
fi
[[ -x $TS_EXIT_BIN ]] || { echo "not built: run build.sh"; exit 1; }
args=(--state-dir $TS_EXIT_STATE --hostname $TS_EXIT_HOSTNAME --port $TS_EXIT_PORT)
[[ -n $TS_EXIT_TAG ]] && args+=(--tag $TS_EXIT_TAG)
# The key is only needed until the identity in the state dir is registered. Without a key file,
# log in interactively: the log shows a URL to approve in a browser (node owned by your user).
if [[ ! -f $TS_EXIT_STATE/keys.json && -f $TS_EXIT_KEYFILE ]]; then
  args+=(--auth-key-file $TS_EXIT_KEYFILE)
else
  args+=(--interactive)   # if control still wants a login, wait for it (URL in the log)
fi
TS_RS_EXPERIMENT=this_is_unstable_software RUST_LOG=${RUST_LOG:-info} \
  nohup $TS_EXIT_BIN "${args[@]}" "$@" >> $TS_EXIT_LOG 2>&1 &
echo $! > $TS_EXIT_PIDFILE
echo "started pid $! ; log: $TS_EXIT_LOG"
