# Brief 21: layered branches on the real GeiserX main

The 97 input commits are represented by 36 feature commits on the real `origin/main` (`3951752`). No branch or remote ref that existed before this task was deleted, rewritten, or pushed. Only the task-created `ts-exit/new-*` refs were finalized during assembly. `ts-exit/new-main` is exactly `ts-exit/new-perf`.

Working tree: `/Volumes/git/ts-exit/scratch/wt-codex-reorg`. All additional validation worktrees and logs are under `/Volumes/git/ts-exit/scratch/`. This report is outside the code repository.

## Layers

| Branch | Head | Commits added in layer | Cumulative commits above origin/main |
|---|---|---:|---:|
| `ts-exit/new-fixes` | `a57c0ca4c2a87d1351fe8bb1e2ce37f2bb0f173d` | 13 | 13 |
| `ts-exit/new-exit-node` | `26cfad86c0a65327923371d3825c93a72e1b1740` | 5 | 18 |
| `ts-exit/new-perf` | `e909d0bb4b9dc9fc4f16fcccdbaecae9c7bf173a` | 12 | 30 |
| `ts-exit/new-bench` | `a9cf235f412dba4028e4ef1abd86395312872ab0` | 4 | 34 |
| `ts-exit/new-experiments` | `4cbe07a3debe22f612a201479543c07f2f0b1840` | 2 | 36 |
| `ts-exit/new-main` | `e909d0bb4b9dc9fc4f16fcccdbaecae9c7bf173a` | alias of new-perf | 30 |

### Features: one row per feature commit

| Layer | Commit | Feature |
|---|---|---|
| fixes | `3c073136ff3b` | fix(forwarder): confine host exit flows to internet destinations |
| fixes | `16e3304be76f` | fix(netstack): make waking pipe try_recv non-blocking |
| fixes | `87bca4de3ff1` | fix(tunnel): revoke sessions after static key or PSK changes |
| fixes | `10c9b13ee3ad` | fix(runtime): remove expired peers from tunnel and routes |
| fixes | `4574e3badb34` | fix(runtime): answer peer queries on self-only netmaps |
| fixes | `b1604483d681` | fix(control): send complete Hostinfo on every map request |
| fixes | `4d7e894728d3` | fix(runtime): retain DNS config and self node across deltas |
| fixes | `19f8808d0524` | fix(runtime): order security updates and supervise forwarding consumers |
| fixes | `2e04ae17a181` | fix(netstack): vendor smoltcp fast-retransmit RTO recovery |
| fixes | `9634ee19f814` | fix(magicsock): match Go betterAddr path scoring |
| fixes | `5308556e899d` | fix(netstack): preserve data after abandoned receives |
| fixes | `60ab7a892baa` | fix(netstack): guarantee FIN and Close delivery by default |
| fixes | `a57c0ca4c2a8` | fix(netstack): remove reset half-open listener sockets |
| exit-node | `3345d74b6aba` | feat(forwarder): gate all-port ingress with owned generations and UDP keepalive |
| exit-node | `0cf0cbc4f780` | feat(runtime): reject IPv6 forwarding with bounded responses |
| exit-node | `307279b8419c` | feat(control): advertise both default routes for exit nodes |
| exit-node | `7dc3815c78e7` | feat(runtime): resolve exit-node DNS through host resolver and peerAPI |
| exit-node | `26cfad86c0a6` | feat(examples): add interactive exit node with DNS and four workers |
| perf | `df5b9448876a` | perf: add an optimized exitnode Cargo profile |
| perf | `23c898b12763` | perf: keep packet-path spans at trace level |
| perf | `d8db8d1da8d0` | perf(runtime): batch netstack egress without copying |
| perf | `c26cecd59749` | perf(magicsock): provision seven MiB UDP socket buffers |
| perf | `435e29a84e9f` | perf(tunnel): add opt-in ring AEAD with transport benchmarks |
| perf | `124d94920012` | perf(magicsock): add connected Apple UDP paths and sendmsg_x batches |
| perf | `341e6af15e30` | perf(netstack): validate socket generations and bound orphan sweeps |
| perf | `8c7986bae2ed` | perf(netstack): replay only socket waiters whose state changed |
| perf | `df411dc1905b` | perf(rx): batch kernel receives and bound only receive-side work |
| perf | `023676a3c50f` | perf(rx): reuse receive storage and remove redundant copies |
| perf | `087d9cb38ac6` | perf(forwarder): coalesce ready TCP bytes and splice host writes |
| perf | `e909d0bb4b9d` | perf: keep Apple batching defaults and bound Linux handoffs and bursts |
| bench | `1a98c935b121` | bench(forwarder): add finite throughput, port-mode and loss workloads |
| bench | `f5b2be8a6382` | bench: add bounded A/B workloads and independent latency probes |
| bench | `1c1d55f18d4c` | bench(forwarder): allow explicit local sink redirects for benchmark addresses |
| bench | `a9cf235f412d` | bench: add authenticated synthetic peer with persistent sessions and safe teardown |
| experiments | `b26c863cd857` | experiment: restore opt-in pipeline, work budgets and forwarder sharding |
| experiments | `4cbe07a3debe` | docs: preserve internal experiment and benchmark reports |

## Reconstruction decisions

- The 13 standalone fix commits carry exit confinement, non-blocking receive, key-change revocation, expired-peer removal, self-only peer queries, complete Hostinfo, retained DNS views, ordered security delivery with supervision, smoltcp recovery and warning settings, betterAddr scoring, abandoned receive, reliable default teardown delivery, and reset half-open cleanup.
- The cancelled-task assertion is in layer 2 with its gated-forwarder test: that test cannot compile without the gate. Port-generation ownership, all-port first-packet admission, UDP reply keepalive, and the first-millisecond reaper correction form one feature commit.
- The runner is re-cut from its final pre-pipeline configuration: interactive login, pending-login wait, bounded unconditional status, host DNS and peerAPI DoH, four workers, and tunnel/echo sinks. Formatting and worker-count churn are squashed.
- RX batching includes syscall batching, ready peer batches, deferred ingress wakes, admission bounds, and receive-only netstack bounds. Full command/egress work budgeting, ACK publication, thread placement, and shard-only APIs remain in experiments. Commands and egress still drain normally in new-main. The receive cleanup preserves borrowed device TxTokens and the existing by-value sender API.
- Ring AEAD and its benchmark are in layer 3. Replay defaults on everywhere; receive batching and splice coalescing default on only on Apple. Linux handoff and host-burst limits remain behind the existing batching/coalescing flags.
- The benchmark peer uses a receive-only bound in layer 4 because the full-budget API first exists in layer 5. The experiments layer restores the original full-budget call. STOP wait uses the transfer timeout, and post-measurement teardown is best effort.
- The experimental runtime is reconstructed as one coherent commit because pipeline placement, security supervision, shard admission/ownership, batch handoffs, and diagnostics share constructors and types. The commit keeps the shared constructors, owners, and diagnostics in one validated tree. Internal codex reports are a separate final commit and occur only in layer 5.
- Every new commit preserves the source author identity and the first included source author date, retains all co-author trailers from its source group, and has `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`. New committer identity is Kasper Nielsen. Upstream history and its existing signatures are left intact.
- Newly authored unicast address fixtures and the runner status-route probe use `203.0.113.0/24`. The multicast refusal fixture remains multicast. Existing upstream/vendor protocol fixtures are inherited; no real endpoint or new public unicast address was authored. The patched vendored TCP source also receives nightly rustfmt.
- Test-only lexical scopes replace explicit drops of a future without a Drop implementation, and the receive-only pipe test imports its trait explicitly. The unchanged metrics re-export is also moved before the facade test module. These corrections make the re-cut layers compile and pass strict all-target clippy independently.

## Old to new mapping

All 93 commits in `24c18fc..ts-exit/linux-fixes` and the four commits in `ts-exit/consolidated..ts-exit/shard-stage1` are accounted for. No feature is silently dropped. A source listed in two layers was split; multiple sources mapping to one new hash were squashed.

| Old commit | Original subject | New commit(s) / layer | Treatment and reason |
|---|---|---|---|
| `824f2d2` | ts_forwarder: confine HostExitDialer exit flows to internet destinations | `3c073136ff3b` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `4d5f234` | ts_forwarder: add forward_throughput bench with RTT emulation | `1a98c935b121` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `4090293` | examples: add exit_node runner | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `99d4474` | ts_netstack_smoltcp: make WakingPipeReceiver::try_recv non-blocking | `16e3304be76f` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `6dc79b9` | style: rustfmt (nightly) the exit_node example and dialer tests | `26cfad86c0a6` exit-node | Formatting-only churn absorbed into the runner and touched-file nightly formatting. |
| `417dd29` | ts_forwarder: gate all-port ingress instead of losing the first packet | `3345d74b6aba` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `50cce10` | ts_forwarder: bench the three port modes and cold-connect latency | `1a98c935b121` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `8b1d19a` | ts_forwarder: add reject_ipv6, a stateless IPv6 rejection helper | `0cf0cbc4f780` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `4f82596` | ts_runtime: gate forwarder ingress and reject IPv6 before the netstack | `0cf0cbc4f780` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `c49d946` | Advertise ::/0 alongside 0.0.0.0/0 for an exit node | `307279b8419c` exit-node | Re-cut on the real upstream base; retained as a standalone feature. |
| `af993f6` | ts_forwarder: keep UDP flows alive while replies flow | `3345d74b6aba` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `9d21497` | ts_forwarder: make the gated port lifecycle generation-safe and owned | `3345d74b6aba` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `d89780d` | ts_runtime: rate-limit the forwarder's IPv6 rejections | `0cf0cbc4f780` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `83f6076` | ts_tunnel: revoke sessions when a peer's static key or PSK changes | `87bca4de3ff1` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `3aba14b` | ts_runtime: remove expired peers from the tunnel and installed routes | `10c9b13ee3ad` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `acb0f06` | ts_forwarder: assert the cancelled forwarder task was cancelled | `3345d74b6aba` exit-node | Squashed into the gate feature in layer 2: the cancellation test depends on that gate. |
| `5ea3190` | examples/exit_node: add --interactive login | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `9cc8964` | examples/exit_node: keep waiting while login is pending | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `eb186e7` | ts_runtime: answer peer queries once a self-only netmap arrives | `4574e3badb34` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `29a588e` | examples/exit_node: unconditional status lines, bounded status query | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `771ad65` | Answer exit-node DNS for unreachable resolvers from this host's resolver | `7dc3815c78e7` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `28684af` | examples/exit_node: answer users' DNS with this host's resolver | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `3aa1772` | Serve exit-node DNS: expose peerapi_port, resolve via the host resolver | `7dc3815c78e7` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `d8250d8` | examples/exit_node: serve exit-node DNS (peerAPI on 41643) | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `5c35974` | ts_control: send the complete Hostinfo on every map request | `b1604483d681` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `5d6b650` | ts_runtime: keep the DNS view's config and self node across deltas | `4d7e894728d3` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `91d2909` | ts_forwarder: emulate packet loss in forward_throughput | `1a98c935b121` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `63c1e98` | Make security bus delivery ordered and supervise forwarding consumers | `19f8808d0524` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `758b79a` | Bound lossy forwarder benchmarks and add a finite-transfer check | `1a98c935b121` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `acca2d9` | Vendor smoltcp 0.13.1 for a narrow TCP recovery backport | `2e04ae17a181` fixes | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `a2327e3` | Backport smoltcp fast-retransmit RTO accounting fix | `2e04ae17a181` fixes | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `0db91e6` | examples/exit_node: tunnel-only benchmark sink | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `30d551a` | Add an exitnode build profile (opt-level 3, symbols kept) | `df5b9448876a` perf | Re-cut on the real upstream base; retained as a standalone feature. |
| `80e78b9` | Make packet-path tracing spans trace-level | `23c898b12763` perf | Re-cut on the real upstream base; retained as a standalone feature. |
| `3a5f0d7` | ts_runtime: hand netstack egress to the dataplane in batches, without copying | `d8db8d1da8d0` perf | Re-cut on the real upstream base; retained as a standalone feature. |
| `6e21f8a` | ts_magicsock: raise underlay UDP socket buffers to 7 MiB | `c26cecd59749` perf | Re-cut on the real upstream base; retained as a standalone feature. |
| `0ecaaca` | ts_magicsock: choose the best path with Go's full betterAddr scoring | `9634ee19f814` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `3196e10` | Add opt-in ring AEAD for WireGuard transport data | `435e29a84e9f` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `378fab5` | Benchmark transport session AEAD and ring tag verification | `435e29a84e9f` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `5cf4baf` | examples/exit_node: echo mode in the benchmark sink (latency probes) | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `e24457d` | examples/exit_node: --worker-threads, default 2 | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `b9972f7` | vendor/smoltcp: silence upstream's own unused-code warnings | `2e04ae17a181` fixes | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `c4dffbe` | examples/exit_node: default --worker-threads to 4, not 2 | `26cfad86c0a6` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `d918b16` | ts_magicsock: connected UDP sockets per busy direct path (Apple, opt-in) | `124d94920012` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `bfb17c1` | ts_magicsock: batched sends with sendmsg_x on connected sockets (opt-in) | `124d94920012` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `ba189ab` | Isolate packet pipeline on an opt-in dedicated Tokio thread | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `a31f0a0` | Add opt-in pipeline batch, admission and thread CPU counters | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `8f6729e` | Bound experimental packet drains and yield between netstack turns | `df411dc1905b` perf<br>`b26c863cd857` experiments | Split by dependency: product/benchmark portion is earlier; experimental or internal-report portion is deferred. |
| `e92ed02` | Coalesce queued direct receive packets into bounded peer batches | `df411dc1905b` perf<br>`b26c863cd857` experiments | Split by dependency: product/benchmark portion is earlier; experimental or internal-report portion is deferred. |
| `dce045f` | Resolve DERP hostnames on the caller and bound packet runtime shutdown | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `fa8c9cd` | Keep security death supervision independent of packet executor load | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `3300a85` | Add bounded A/B workloads with per-flow accounting and separate probes | `f5b2be8a6382` bench | Re-cut on the real upstream base; retained as a standalone feature. |
| `4c8c372` | Add an independent authenticated WireGuard and TCP load peer | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `b7216a6` | Benchmark-only exit redirect to a local sink (TS_BENCH_EXIT_REDIRECT) | `1c1d55f18d4c` bench<br>`b26c863cd857` experiments | Split by dependency: product/benchmark portion is earlier; experimental or internal-report portion is deferred. |
| `8a2a0e1` | examples/synthetic_peer: enroll with a login link instead of an auth key | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `ad10468` | examples/synthetic_peer: advertise a LAN endpoint through control | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `a55d5e2` | ts_forwarder: don't reap a UDP flow in its port's first millisecond | `3345d74b6aba` exit-node | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `13d4c4c` | ts_netstack_smoltcp_core: don't let an abandoned receive eat data | `5308556e899d` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `4e6490d` | ts_netstack_smoltcp: never drop a FIN or Close on a full command channel | `60ab7a892baa` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `97e4934` | examples/synthetic_peer: report a stalled flow's progress on timeout | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `92d2c78` | examples/synthetic_peer: hold one control session for a whole matrix | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `818deb2` | examples/synthetic_peer: report the enrolled hostname from hold sessions | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `664ae33` | ts_netstack_smoltcp_core: validate socket kinds through the generation registry | `341e6af15e30` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `c2b3dbe` | ts_netstack_smoltcp_core: bound orphan sweeps with an idle reclamation deadline | `341e6af15e30` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `1292587` | examples/exit_node: report netstack replay and sweep counters | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `0684e38` | docs: record netstack scan fixes and benchmark handoff | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `de3a6bf` | ts_netstack_smoltcp_core: add opt-in targeted waiter replay | `8c7986bae2ed` perf | Re-cut on the real upstream base; retained as a standalone feature. |
| `ebf893d` | docs: record targeted replay design and validation | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `12f705c` | ts_netstack_smoltcp_core: remove half-open sockets a RST sent back to LISTEN | `a57c0ca4c2a8` fixes | Re-cut on the real upstream base; retained as a standalone feature. |
| `3564b87` | netstack: add opt-in turn-boundary ACK egress publication | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `b20e50b` | magicsock: add opt-in bounded receive batches through netstack admission | `df411dc1905b` perf<br>`b26c863cd857` experiments | Split by dependency: product/benchmark portion is earlier; experimental or internal-report portion is deferred. |
| `7d336bb` | upload: reuse receive storage and remove redundant copies and tracing | `023676a3c50f` perf<br>`b26c863cd857` experiments | Split by dependency: product/benchmark portion is earlier; experimental or internal-report portion is deferred. |
| `27cc9cf` | docs: record receive and ACK batching validation and profile correction | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `9251a6b` | netstack: decouple full work budgeting from ACK and RX batching | `df411dc1905b` perf<br>`b26c863cd857` experiments | Split by dependency: product/benchmark portion is earlier; experimental or internal-report portion is deferred. |
| `85dd5f5` | docs: record budget separation, download diagnosis and next flag matrix | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `dbb32f2` | netstack: add opt-in shard EOF handling and ingress discard barrier | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `fb234c6` | forwarder: share shard admission and supervise colocated relay tasks | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `5597622` | dataplane: preserve authenticated identity for opt-in exit dispatch | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `310e9a3` | netstack: bound shard output and expose actual TCP buffer occupancy | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `6b40ef2` | dataplane: add bounded shard egress to the existing WG owner | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `66db0f1` | runtime: add opt-in authenticated exit-flow shards with fail-closed ownership | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `026886f` | runtime: synchronize authenticated batches with shard security barriers | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `0753c5c` | docs: record stage-1 ownership, security limits and Claude scaling matrix | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `0698abe` | netstack: collect ready TCP bytes across ring wrap in one receive | `087d9cb38ac6` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `6c08daa` | forwarder: add opt-in ready-only splice coalescing and host write counters | `087d9cb38ac6` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `31d6f76` | runtime: reuse the shard pump's one-packet revalidation buffer | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `a11c9f1` | runtime: add an isolated shard splice write-size and TCP RTT probe | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `6ea505a` | docs: report splice coalescing measurements and the next small A/B | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `e59e731` | Enable targeted replay, RX batching and splice coalescing by default | `e909d0bb4b9d` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `3e9ee27` | Default RX batching and splice coalescing on only on Apple platforms | `e909d0bb4b9d` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `f01ba7c` | runtime: bound Linux RX handoffs while retaining recvmmsg batches | `e909d0bb4b9d` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `39a6220` | forwarder: bound Linux download bursts in flagged TCP splices | `e909d0bb4b9d` perf | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `0a70e1b` | docs: report Linux batching candidates and the two-pass A/B matrix | `4cbe07a3debe` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `b6acf2a` | perf: add bounded ready-batch merging and inline netstack egress hooks | `b26c863cd857` experiments | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `41631e2` | perf: shard handoff metrics, host-write and offered-packet histograms, probe policy | `b26c863cd857` experiments<br>`4cbe07a3debe` experiments | Split code/metrics from the internal report; both remain in layer 5. |
| `6819586` | examples/synthetic_peer: wait for STOP as long as the transfer timeout | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |
| `1949151` | examples/synthetic_peer: make post-measurement teardown best-effort | `a9cf235f412d` bench | Squashed into the final feature with its follow-ups; avoids replaying intermediate configuration, formatting, or self-fix churn. |

## Validation

All Cargo commands use Rust 1.95.0 and `--locked`. Builds are workspace-wide. Clippy checks the touched crates and all targets with `-D warnings`, which also compiles benchmark/example targets. Nightly rustfmt checks every authored/touched Rust file, including the patched vendored TCP source; unmodified third-party import files are excluded from repository-style formatting.

Final validation worktrees are `scratch/wt-codex-reorg-verified-{layer}`. Each has an isolated target directory and records its exact head hash. Preliminary assembly logs are retained for diagnosis and are not used as final pass evidence.

| Layer | Workspace build | Workspace tests and doctests | Touched-file fmt | Strict clippy | Required crate suite / flag modes |
|---|---|---|---|---|---|
| fixes | PASS; 200.0 s | PASS; 834.6 s | PASS; 0.3 s | PASS; 28.7 s | default: PASS |
| exit-node | PASS; 226.0 s | PASS; 853.9 s | PASS; 0.3 s | PASS; 39.7 s | default: PASS |
| perf | PASS; 207.4 s | PASS; 861.5 s | PASS; 0.4 s | PASS; 37.8 s | default: PASS<br>flags-0: PASS<br>flags-1: PASS |
| bench | PASS; 203.4 s | PASS; 891.0 s | PASS; 0.5 s | PASS; 42.0 s | default: PASS<br>flags-0: PASS<br>flags-1: PASS |
| experiments | PASS; 10.5 s | PASS; 134.9 s | PASS; 0.5 s | PASS; 8.0 s | default: PASS<br>flags-0: PASS<br>flags-1: PASS<br>ack-budget: PASS<br>pipeline: PASS<br>shards-4: PASS<br>shards-4-batch: PASS |

**origin/main baseline:** build: PASS (140.2 s), workspace-tests: PASS (475.5 s), clippy: PASS (47.1 s). No workspace failure was observed on origin/main.

Required suite: `geiserx_ts_netstack_smoltcp`, `geiserx_ts_netstack_smoltcp_core`, `geiserx_ts_netstack_smoltcp_socket`, `geiserx_ts_forwarder`, `geiserx_ts_runtime`, `geiserx_ts_magicsock`, `geiserx_ts_tunnel`, `geiserx_ts_dataplane`. The `geiserx_ts_runtime/ring-aead` feature is enabled from layer 3 onward, where that feature first exists; layers 1 and 2 run the same crate suite without the not-yet-added feature.

Perf and bench test defaults plus forced 0 and 1 for replay/RX/coalescing. Experiments additionally test ACK + full budget, pipeline placement, N=4 shards, and N=4 shard batching with metrics. Pipeline placement and N>1 are tested separately because their combination is deliberately refused. Full command lines and environment overrides are in `scratch/reorg-21/verified-logs/{layer}/*.log` and `results.json`.

Additional checks passed: matrix2 loopback Python tests (2 tests), synthetic-peer ring-AEAD fixture (8 finite flows in each direction, 8,000,000 bytes per direction), forwarding repeats on fixes (12 tests) and exit-node (16 tests), and the product runner under the optimized exitnode profile with ring AEAD. Logs: `scratch/reorg-21/matrix2-tests.log`, `synthetic-bench-tests.log`, `forwarding-repeat-{fixes,exit-node}.log`, and `product-exitnode-profile.log`.

## Product equivalence and every remaining difference

The requested comparison includes the two real upstream commits that were not present in linux-fixes. Their ServeConfig change and parity audit are retained intentionally.

```text
Cargo.lock                                         |    8 -
 Cargo.toml                                         |    8 -
 PARITY_AUDIT.json                                  |   25 +-
 docs/codex-13-netstack-scans.md                    |  155 --
 docs/codex-14-targeted-replay.md                   |  168 --
 docs/codex-15-rx-ack-batching.md                   |  197 ---
 docs/codex-16-decouple-budget.md                   |  235 ---
 docs/codex-17-shard-stage1.md                      |  347 -----
 docs/codex-18-splice-coalesce.md                   |  325 ----
 docs/codex-20-linux-fixes.md                       |  367 -----
 examples/exit_node/main.rs                         |   24 +-
 examples/synthetic_peer.rs                         | 1017 -------------
 runner/.gitignore                                  |    1 -
 runner/ab-matrix2.sh                               |   51 -
 runner/matrix2.py                                  |  191 ---
 runner/sink-server.py                              |   53 -
 runner/synthetic-matrix.py                         |  111 --
 runner/test_matrix2.py                             |   68 -
 runner/test_synthetic.py                           |   68 -
 src/error.rs                                       |   11 -
 src/lib.rs                                         |  106 +-
 ts_control/src/lib.rs                              |    6 +-
 ts_control/src/serve.rs                            |  135 +-
 ts_control/src/serve/ipn.rs                        | 1609 ++++++++++++++++++++
 ts_dataplane/src/async_tokio.rs                    |   55 -
 ts_dataplane/src/lib.rs                            |  202 +--
 ts_derp/src/client.rs                              |   27 +-
 ts_derp/src/dial.rs                                |  193 +--
 ts_forwarder/Cargo.toml                            |    4 -
 ts_forwarder/benches/forward_throughput.rs         |  366 -----
 ts_forwarder/src/all_port.rs                       |   98 +-
 ts_forwarder/src/dialer.rs                         |  113 +-
 ts_forwarder/src/forwarder.rs                      |   67 +-
 ts_forwarder/src/lib.rs                            |    8 +-
 ts_forwarder/src/port_gate.rs                      |   42 -
 ts_forwarder/src/shard_resources.rs                |  153 --
 ts_forwarder/src/tcp.rs                            |   20 +-
 ts_forwarder/src/udp.rs                            |    6 +-
 ts_forwarder/tests/forwarding.rs                   |    5 -
 ts_magicsock/src/batch.rs                          |   28 +-
 ts_magicsock/src/connected.rs                      |   92 --
 ts_magicsock/src/lib.rs                            |   17 +-
 ts_magicsock/src/sock.rs                           |    7 -
 ts_netstack_smoltcp/src/lib.rs                     |   45 +-
 ts_netstack_smoltcp/src/pipe.rs                    |  396 +----
 ts_netstack_smoltcp/src/run.rs                     |    2 +-
 ts_netstack_smoltcp/tests/budget.rs                |   15 +-
 ts_netstack_smoltcp_core/src/lib.rs                |  128 +-
 ts_netstack_smoltcp_core/src/pipeline_metrics.rs   |   16 +-
 ts_netstack_smoltcp_core/src/registry.rs           |    4 -
 .../src/socket_impl/tcp/listener.rs                |   10 +-
 .../src/socket_impl/tcp/stream.rs                  |   14 +-
 ts_netstack_smoltcp_core/src/targeted_replay.rs    |   35 +-
 ts_netstack_smoltcp_core/src/wake_device.rs        |    3 -
 ts_runtime/src/dataplane.rs                        |  108 +-
 ts_runtime/src/direct.rs                           |   82 +-
 ts_runtime/src/env.rs                              |   33 -
 ts_runtime/src/error.rs                            |   20 -
 ts_runtime/src/forwarder_actor.rs                  |  265 +---
 ts_runtime/src/forwarder_shards.rs                 |  763 ----------
 ts_runtime/src/forwarder_shards/fragments.rs       |  266 ----
 ts_runtime/src/forwarder_shards/tests.rs           |  594 --------
 .../src/forwarder_shards/tests/splice_probe.rs     |  224 ---
 ts_runtime/src/lib.rs                              |  136 +-
 ts_runtime/src/multiderp.rs                        |   14 +-
 ts_runtime/src/netstack_actor.rs                   |   38 +-
 ts_runtime/src/pipeline.rs                         |  408 -----
 ts_runtime/src/pipeline_metrics.rs                 |  100 --
 ts_runtime/src/security.rs                         |  120 +-
 ts_runtime/src/serve.rs                            |   19 +-
 ts_tunnel/src/transport_aead.rs                    |    9 +-
 vendor/smoltcp/src/socket/tcp.rs                   |   38 +-
 72 files changed, 2109 insertions(+), 8585 deletions(-)
```

Per-file explanation follows. All normal-path replay, receive batching, splice, connected UDP, AEAD selection, buffer sizes, forwarding security, DNS, and runner defaults are retained. Removed flags and APIs belong to layers 4–5. This is source-path equivalence backed by regression tests, not a new live throughput A/B measurement.

| File | Explanation |
|---|---|
| `Cargo.lock` | Remove benchmark-only root dependency edges; keep the vendored smoltcp lock entry and ring dependencies. |
| `Cargo.toml` | Remove forward-throughput/synthetic-peer benchmark target declarations and dependencies. Keep exit-node runner, ring feature, and exitnode profile. |
| `PARITY_AUDIT.json` | Retain the real upstream #591 parity-audit changes. |
| `docs/codex-13-netstack-scans.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `docs/codex-14-targeted-replay.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `docs/codex-15-rx-ack-batching.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `docs/codex-16-decouple-budget.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `docs/codex-17-shard-stage1.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `docs/codex-18-splice-coalesce.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `docs/codex-20-linux-fixes.md` | Internal report moved to layer 5; none of these reports occur in layers 1–4. |
| `examples/exit_node/main.rs` | Remove experimental aggregate status/counter logging; retain final runner behavior and both sinks. Status timeout route-query fixture uses a documentation address. |
| `examples/synthetic_peer.rs` | Benchmark-only tool; moved to layer 4 with all source fixes and STOP/teardown additions. |
| `runner/.gitignore` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `runner/ab-matrix2.sh` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `runner/matrix2.py` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `runner/sink-server.py` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `runner/synthetic-matrix.py` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `runner/test_matrix2.py` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `runner/test_synthetic.py` | Bounded A/B, sink, synthetic workload and loopback-test tooling moved to layer 4. |
| `src/error.rs` | Remove experimental runtime error variants and conversions. |
| `src/lib.rs` | Retain real upstream ServeConfig API/schema changes; remove experimental metrics re-export and runtime/shard error plumbing. |
| `ts_control/src/lib.rs` | Retain the real upstream ServeConfig wire-shape exports. |
| `ts_control/src/serve.rs` | Retain real upstream Go ipn.ServeConfig serialization, moved to serve/ipn.rs. |
| `ts_control/src/serve/ipn.rs` | Real upstream Go ipn.ServeConfig wire implementation added by #588. |
| `ts_dataplane/src/async_tokio.rs` | Remove experimental cooperative runner, authenticated shard admission/barriers, and shard-specific dispatch behavior. |
| `ts_dataplane/src/lib.rs` | Remove opt-in authenticated shard dispatch/identity and barrier hooks; preserve the single WG owner normal data path. |
| `ts_derp/src/client.rs` | Defer caller-resolution API plumbing used by the isolated packet runtime. |
| `ts_derp/src/dial.rs` | Defer resolved-dial split and runtime isolation test used by pipeline placement; ordinary dial remains. |
| `ts_forwarder/Cargo.toml` | Move forward_throughput bench declaration to layer 4. |
| `ts_forwarder/benches/forward_throughput.rs` | Move complete throughput/port/loss/finite-transfer tooling to layer 4. |
| `ts_forwarder/src/all_port.rs` | Remove shared shard resource/task-spawner scaffolding; retain owned port generations, node-wide limits for the single stack, and UDP reply keepalive. |
| `ts_forwarder/src/dialer.rs` | Remove benchmark sink redirect; replace inherited unicast fixtures with documentation addresses. Keep exit confinement and host-resolver DNS. |
| `ts_forwarder/src/forwarder.rs` | Remove shared admission and colocated shard task-spawner plumbing; retain the single forwarder and gate. |
| `ts_forwarder/src/lib.rs` | Remove shard resource/snapshot exports and benchmark dialer export; retain gate, IPv6 rejection, splice, and splice metrics. |
| `ts_forwarder/src/port_gate.rs` | Remove shard security-barrier discard and occupancy snapshot methods/tests; keep deferred RX admission, port generations, and final defaults. |
| `ts_forwarder/src/shard_resources.rs` | Experimental global admission/task-spawner implementation; restored only in layer 5. |
| `ts_forwarder/src/tcp.rs` | Remove shard/global task supervision plumbing; retain coalesced reads/writes, counters, and Linux burst bounds. |
| `ts_forwarder/src/udp.rs` | Remove colocated shard task spawning; preserve UDP flow lifetime and replies. |
| `ts_forwarder/tests/forwarding.rs` | Remove ACK/full-budget mode setup; retain RX receive bounds, cold-port tests, cancellation assertion, half-close/coalescing tests, and platform defaults. |
| `ts_magicsock/src/batch.rs` | Remove experimental offered/completed send histograms; retain connected sendmsg_x implementation. |
| `ts_magicsock/src/connected.rs` | Remove alternate packet-runtime handle and placement test; retain connected sockets, batched receive, and reusable storage. |
| `ts_magicsock/src/lib.rs` | Remove experiment-only histogram exports and benchmark seal_pong export; retain receive-batch counters. |
| `ts_magicsock/src/sock.rs` | Remove packet-runtime handle setter; keep final receive batching, owned buffers, peer attribution, and FIFO bounds. |
| `ts_netstack_smoltcp/src/lib.rs` | Remove full-work, ACK publication, and shard-only API wrappers; retain receive-only bound and normal APIs. |
| `ts_netstack_smoltcp/src/pipe.rs` | Remove ACK batch envelopes, pending egress buffers, turn flush hooks, shard inline hooks, and associated tests. Keep non-blocking try_recv, byte/count bounds, deferred RX wakes, borrowed TxTokens, and by-value compatibility. Explicit trait import makes the receive-only test independent. |
| `ts_netstack_smoltcp/src/run.rs` | Remove full-work-budget activation; yield after receive-bound exhaustion only. |
| `ts_netstack_smoltcp/tests/budget.rs` | Retain receive-only fairness test; defer the full-budget variant. |
| `ts_netstack_smoltcp_core/src/lib.rs` | Remove full command/transmit budgeting, transmit-token limiter, shard EOF/discard hooks, and shard occupancy APIs; preserve registry, bounded orphan sweeps, replay, and receive-only bounds. |
| `ts_netstack_smoltcp_core/src/pipeline_metrics.rs` | Remove full-egress-budget counter; retain cheap replay/orphan/RX/egress-pass counters needed by proven work. |
| `ts_netstack_smoltcp_core/src/registry.rs` | Remove shard shared TCP-counter assertions; retain generation/type registry assertions. |
| `ts_netstack_smoltcp_core/src/socket_impl/tcp/listener.rs` | Remove opt-in shard CloseWait acceptance; retain reset half-open cleanup and existing listener semantics. |
| `ts_netstack_smoltcp_core/src/socket_impl/tcp/stream.rs` | Use test-only lexical scopes instead of drop(non-Drop future); production stream/reaping code is retained. |
| `ts_netstack_smoltcp_core/src/targeted_replay.rs` | Remove full-budget comparison cases; retain all replay behavior, ring-wrap/coalesced-read guards, receive-only bounds and unbounded-egress tests. |
| `ts_netstack_smoltcp_core/src/wake_device.rs` | Remove ACK turn-end publication hook; retain original async readiness contract. |
| `ts_runtime/src/dataplane.rs` | Remove isolated-runtime runner choice and shard dispatch/security barrier construction; retain session revocation and expiry removal. |
| `ts_runtime/src/direct.rs` | Remove packet/background runtime placement; retain final Apple RX defaults and Linux per-peer FIFO handoff bounds. |
| `ts_runtime/src/env.rs` | Remove alternate runtime handles and placement helpers; retain guaranteed ordered bus and forwarding/DNS config. |
| `ts_runtime/src/error.rs` | Remove experiment-only pipeline/shard runtime error variants. |
| `ts_runtime/src/forwarder_actor.rs` | Remove pipeline placement, full budgets, shard dispatch/diagnostics, and benchmark redirect; retain receive-only admission, egress batches, gate, DNS, and IPv6 reject rate limit. |
| `ts_runtime/src/forwarder_shards.rs` | Experimental authenticated ownership/dispatch/supervision; removed from product and restored in layer 5. |
| `ts_runtime/src/forwarder_shards/fragments.rs` | Experimental fragment-to-shard ownership and bounds; restored in layer 5. |
| `ts_runtime/src/forwarder_shards/tests.rs` | Experimental shard security/admission/supervision regressions; restored in layer 5. |
| `ts_runtime/src/forwarder_shards/tests/splice_probe.rs` | Experimental shard write-size/RTT diagnostic probe; restored in layer 5. |
| `ts_runtime/src/lib.rs` | Remove pipeline/shard construction, alternate security monitor placement, and counters; retain original security guard plus real upstream ServeConfig changes. |
| `ts_runtime/src/multiderp.rs` | Remove caller-resolution/alternate-runtime split required by pipeline placement. |
| `ts_runtime/src/netstack_actor.rs` | Remove ACK/full-budget setup, isolated-runtime spawns, and pipeline counters; retain RX-only bounds, wake batching, and zero-copy bounded egress batches. |
| `ts_runtime/src/pipeline.rs` | Experimental dedicated packet/security runtime and associated probes; restored in layer 5. |
| `ts_runtime/src/pipeline_metrics.rs` | Experimental aggregate counters/status snapshot; restored in layer 5. |
| `ts_runtime/src/security.rs` | Remove alternate security runtime handle and isolation tests; retain ordered bus startup barrier and fail-closed actor/pump supervision. |
| `ts_runtime/src/serve.rs` | Retain real upstream ServeConfig serving changes. |
| `ts_tunnel/src/transport_aead.rs` | Nightly import ordering only; AEAD implementation is unchanged. |
| `vendor/smoltcp/src/socket/tcp.rs` | Nightly formatting of the patched vendored file only; fast-retransmit RTO backport is unchanged. |

## Complete experiments equivalence

Expected tree: `b2771053fef87123ed9d9975c416e1a9615c5e9d`. It was independently reconstructed in a detached worktree by applying the full `24c18fc..linux-fixes` tree delta to real origin/main, then applying `b6acf2a`, `41631e2`, `6819586`, and `1949151` without committing. The complete patch applied cleanly, so the two upstream additions remain present.

```text
examples/exit_node/main.rs                         |  2 +-
 examples/synthetic_peer.rs                         | 31 ++++++++++--------
 src/lib.rs                                         |  6 ++--
 ts_derp/src/client.rs                              |  3 +-
 ts_derp/src/dial.rs                                |  3 +-
 ts_forwarder/src/dialer.rs                         | 20 ++++++------
 ts_forwarder/src/shard_resources.rs                |  2 ++
 ts_magicsock/src/batch.rs                          |  4 ++-
 ts_netstack_smoltcp/src/pipe.rs                    |  7 ++--
 .../src/socket_impl/tcp/stream.rs                  | 14 ++++----
 ts_runtime/src/forwarder_shards/tests.rs           |  2 +-
 ts_tunnel/src/transport_aead.rs                    |  9 +++--
 vendor/smoltcp/src/socket/tcp.rs                   | 38 +++++++++++++---------
 13 files changed, 78 insertions(+), 63 deletions(-)
```

Differences against that expected tree are limited to:

- `examples/exit_node/main.rs`, `ts_forwarder/src/dialer.rs`, and `ts_runtime/src/forwarder_shards/tests.rs`: documentation-address substitutions. Multicast/forbidden/private-address fixtures remain intact.
- `ts_forwarder/src/shard_resources.rs`: document and narrowly allow the intentional test-only return of a JoinHandle from the task-local scope; the test must join after its parent scope ends to prove capture. Runtime code and test execution are unchanged.
- `src/lib.rs`: move the unchanged metrics re-export before the test module for strict all-target clippy; no API or runtime change.
- `ts_netstack_smoltcp_core/src/socket_impl/tcp/stream.rs` and `ts_netstack_smoltcp/src/pipe.rs`: test-only lexical scopes for strict clippy; same cancellation/lifetime boundary.
- `examples/synthetic_peer.rs`, `ts_derp/src/client.rs`, `ts_derp/src/dial.rs`, `ts_magicsock/src/batch.rs`, `ts_tunnel/src/transport_aead.rs`, and `vendor/smoltcp/src/socket/tcp.rs`: nightly formatting/import ordering only.

A direct comparison with shard-stage1 also includes the two upstream commits, consolidated defaults, Linux handoff/burst fixes, and the Linux report. A direct comparison with linux-fixes also includes the four requested shard-stage additions. The expected-union comparison above is the complete preservation check.

## Limits and unresolved items

- `bd prime` could not run because Beads (`bd`) is not installed. No tracker or remote sync was attempted.
- Validation runs on this Apple host. The Linux-specific limits are preserved from Brief 20, but no new Linux-host throughput A/B run or cross-platform measurement is claimed.
- No live enrollment, production node restart, push, branch deletion, merge, or publication was performed.
- Imported third-party files retain their upstream fixtures and whitespace. The authored vendored TCP patch passes nightly formatting; unmodified vendor files are not reformatted.
- All final validation checks completed successfully; no unresolved build, test, formatting, or clippy failure remains.
