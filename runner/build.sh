#!/bin/zsh
# Build the runner with the exitnode profile (release with line tables) and the ring AEAD backend.
set -eu
source ${0:A:h}/env.sh
cd $TS_EXIT_FORK
cargo +1.95.0 build --profile exitnode --locked --features ring-aead --example exit_node
ls -la $TS_EXIT_BIN
