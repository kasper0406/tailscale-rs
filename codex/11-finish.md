Your previous run ended mid-task (last action: `cargo check --example synthetic_peer` exited 101; nothing
committed; 28 files modified in scratch/wt-codex on ts-exit/pipeline-split). Your design review
(docs/codex-10-sharding-design.md) is read and accepted as the plan; the user agreed the goal is general
scalability. Please finish Stage 0 only:
 1. Commit the salvage as separate tested commits on ts-exit/pipeline-split (bounded drivers + fairness
    test; receive-bridge coalescing; independent death supervisor; DERP DNS on caller + stalled-resolver
    test; TS_DEBUG_PIPELINE_METRICS counters incl. offered/sendmsg_x batch histograms; runner/ab-matrix2.sh).
    Park the two-stage topology (don't ship it as a mode).
 2. Finish the load generator. Prefer a mode that needs NO tailnet enrollment for routine scaling runs
    (e.g. a fixture where the node under test is started in-process/with a static peer config by the tool,
    using the production forwarder/dataplane path) so we can sweep without touching the user's tailnet. If a
    live mode against the real node needs an auth key/enrollment, keep it but say exactly what the user must
    do; I will not enroll anything without asking. Report the generator's own CPU so we know it has headroom.
 3. Run the full test sets (runtime/dataplane/forwarder/magicsock, ring on; knob unset and 1), clippy, and
    build `--profile exitnode --example exit_node --features ring-aead` + the generator; copy binaries with
    sha256 to scratch/codex-stage0/. README with exact commands for me to run the Stage 0 A/B.
Same rules as before (DCO, own worktree, codex-target, don't touch live node/runner checkout/Go client).
