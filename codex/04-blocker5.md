Thanks: I reviewed and applied your blocker-2 patches (now 83f6076, 3aba14b on ts-exit/runner; pushed). I also fixed
your review findings 2–5 (9d21497: generation-safe gate, JoinSet-owned listeners, atomic Closing claim, open
timeout), rate-limited IPv6 rejections (d89780d), and finding 7 was already fixed in af993f6 (UDP refresh on
replies + live-flow-aware port reaping). Phase 3A (connected sockets + DF, opt-in) is on ts-exit/perf-connected
(19e0507).

Task: implement blocker 5 with the SMALLEST CORRECT fix you proposed in codex-02-ipv6.md §4: Guaranteed ordered
MessageBus delivery with an unbounded broker mailbox, ask-based Register, and a subscriber-ready barrier so no
control/cached-map publication happens before the security subscribers (dataplane, packetfilter, src filter, route
updater, forwarder, peer tracker — verify which ones actually consume security state) are registered. Critical
subscriber death must fail closed (stop forwarding) rather than silently continue. Do NOT attempt the bigger
snapshot redesign; note it as follow-up.
Requirements:
 - Work in your worktree based on ts-exit/runner HEAD (acb0f06). Deliver git format-patch files in
   scratch/codex-blocker5/ with a README (what changed, tests, residual risk). DCO line:
   `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`.
 - Tests: saturate a subscriber mailbox and prove a later revocation (peer deletion / filter delta) is still applied;
   late-subscriber registration ordering; no deadlock under publish-from-handler; subscriber failure fails closed.
 - Run ts_runtime, ts_dataplane, ts_control, ts_forwarder tests (release) and clippy; report the exact counts.
 - Keep it minimal and reviewable. If you find the minimal fix is unsafe (deadlock risk you can't rule out), stop
   and write up why instead of shipping it.
