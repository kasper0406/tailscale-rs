#!/bin/zsh
# Build the runner in release mode for aarch64-apple-darwin.
set -eu
source ${0:A:h}/env.sh
cd $TS_EXIT_FORK
cargo build --release --locked --example exit_node
ls -la $TS_EXIT_BIN
