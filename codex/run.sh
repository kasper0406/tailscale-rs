#!/bin/zsh
# usage: codex/run.sh <brief.md> <log> [resume-session-id]
set -u
cd /Volumes/git/ts-exit
FLAGS=(--skip-git-repo-check -m gpt-6.1-sol -c 'model_reasoning_effort="high"' -s workspace-write -c 'sandbox_workspace_write.network_access=true' -c 'sandbox_workspace_write.writable_roots=["/Volumes/git/ts-exit"]')
if [[ $# -ge 3 ]]; then
  codex exec "${FLAGS[@]}" resume "$3" - < "$1" > "$2" 2>&1
else
  codex exec "${FLAGS[@]}" - < "$1" > "$2" 2>&1
fi
