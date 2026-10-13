# Brief 22: rewrite the commit messages of the layered branches (messages only, no code changes)

Your brief-21 reorganisation left every new commit body as "Re-cut and squash the original feature and its
follow-up fixes for Brief 21." That is meaningless to maintainers, and these commits are now in public PRs
(GeiserX/tailscale-rs#597, #598). Rewrite the messages.

Repo `/Volumes/git/ts-exit/repos/geiserx-tailscale-rs`. Branches (stacked): `ts-exit/fixes` (13) → `exit-node`
(5) → `perf` (12) = `main` → `bench` (4) → `experiments` (2), on `origin/main`. Your old→new mapping is in
`/Volumes/git/ts-exit/docs/REORG-21.md`. The original commits are reachable through the tags
`archive/ts-exit/*` and `local-archive/ts-exit/*` (e.g. `git log archive/ts-exit/linux-fixes`).

Rules:
- Messages only. Every rewritten commit must have exactly the same tree as its original new commit (verify
  with `git rev-parse <commit>^{tree}` for all 36, and report the check).
- Keep the existing subjects unless one is unclear. Body: a few concise lines in the style of the originals:
  what was wrong or missing, what the change does, and anything a reviewer must know (opt-in flags,
  platform specifics, limits). Derive the content from the original commit messages for that feature. No
  references to briefs, codex, Claude, workspace docs, decision IDs (F17/D22…), or internal branch names.
  Keep it short; no change logs of our iteration history.
- Trailers: keep `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>` on every commit, plus a
  `Co-Authored-By:` line for each AI co-author that appeared on the squashed originals
  (`Claude Opus 5.5 <noreply@anthropic.com>` and/or `GPT-6.1 (codex) <noreply@openai.com>`).
- Never add public IP addresses.
- Rewrite the local branches `ts-exit/fixes`, `exit-node`, `perf`, `main`, `bench`, `experiments` in place
  (e.g. `git rebase -i`-free approaches: `git filter-repo` is NOT appropriate here; use a scripted
  `git commit-tree` replay or `git rebase --exec 'git commit --amend -F …'`). Keep the stacking: each layer
  must sit on the rewritten layer below, and `main` must equal `perf`. Do not push and do not touch remotes or
  tags.

Deliver `/Volumes/git/ts-exit/docs/REWORD-22.md`: old→new SHA per commit, the tree-equality check result, and the
full new message of each commit so Claude can review them.
