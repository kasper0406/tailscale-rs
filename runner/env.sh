# Shared settings for the ts-exit runner scripts. Sourced, not executed.
TS_EXIT_ROOT=/Volumes/git/ts-exit
TS_EXIT_FORK=$TS_EXIT_ROOT/repos/geiserx-tailscale-rs
TS_EXIT_BIN=$TS_EXIT_FORK/target/exitnode/examples/exit_node
TS_EXIT_STATE=${TS_EXIT_STATE:-$TS_EXIT_ROOT/state/rust-exit}
TS_EXIT_KEYFILE=${TS_EXIT_KEYFILE:-$HOME/.config/ts-exit/authkey}
TS_EXIT_HOSTNAME=${TS_EXIT_HOSTNAME:-kmini}
# Empty (TS_EXIT_TAG=) means untagged, e.g. for interactive login without tagOwners set up.
TS_EXIT_TAG=${TS_EXIT_TAG-tag:rust-exit}
TS_EXIT_PORT=${TS_EXIT_PORT:-41642}
TS_EXIT_PIDFILE=$TS_EXIT_STATE/exit_node.pid
TS_EXIT_LOG=$TS_EXIT_STATE/exit_node.log
