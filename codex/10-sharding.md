I interrupted you deliberately (your worktree changes on ts-exit/pipeline-split are intact). The profile you
asked for is at scratch/prof/pipeline-single-8.txt (knob=1, eight loaded flows, `sample <pid> 5 1`):
categorized, the pipeline thread is 40% send syscalls, 16% crypto, 10% netstack, 11% malloc/memcpy, tokio/
locks/clock ~2% each.

Direction change from the user: the goal is a GENERALLY scalable design, not tuning for their 750 Mbit/s
home line. A stage (pipeline) split is capped by its slowest stage and fixes the hop count; it is not the
general answer. Proposed general architecture — please critique it adversarially first, then we build
(the user wants your agreement on the design):

  Flow-sharded, run-to-completion ("thread per core"):
  - N shards (configurable; N=1 == today's single pipeline thread), each one sticky thread with a
    current-thread runtime that OWNS: its own smoltcp instance(s) (forwarder any-IP stack, and the app stack
    on shard 0 or sharded too), the per-flow work for its flows (TCP splice / UDP relay host sockets in
    exit mode run on the same shard runtime — no cross-thread hop), encryption of its egress (keys shared
    read-only; per-peer send counter as an atomic, as wireguard-go does) and batched UDP sends.
  - RX: a peer's datagrams arrive on one socket (macOS delivers a connected peer's traffic to that socket;
    see bench/reuseport-check). Receive threads read in batches (recvmsg_x), decrypt (per-peer replay window
    under a short lock or lock-free), then steer each inner packet to shard = hash(5-tuple) — one
    cross-thread hop at most, zero if the receiver is that shard. Consider whether RX/decrypt should itself
    run on shard threads (receive on whichever shard, decrypt, steer) to avoid dedicated RX threads.
  - Fragments: hash on (src, dst, proto) for any packet without ports — or keep reassembly before steering.
  - Per-shard: listeners/port gate replicated or a global gate with per-shard registration; timers; budgets.
  - Control plane (filters/routes/peers) shared read-only, swapped atomically; security supervision must stay
    fail-closed across all shards (your earlier concern about starvation applies per shard).
  - Single heavy flow is inherently one shard core (TCP state machine is serial); RX/decrypt for one heavy
    peer could still be spread across shards.

Tasks:
 1. docs/codex-10-sharding-design.md: critique (what's wrong/missing, e.g. smoltcp any-IP + per-port
    listeners per shard, the app stack, TSMP/ICMP, ordering requirements of WireGuard per peer, DERP path,
    connected sockets and SO_REUSEPORT on macOS vs Linux, memory per shard), agreement/disagreement, and a
    staged plan where every stage is shippable, opt-in and measurable.
 2. Salvage: from your current worktree, commit the design-independent pieces as separate, tested commits on
    ts-exit/pipeline-split (bounded netstack driver with budgets + fairness test; receive-bridge coalescing
    of already-queued packets; DERP UseDns on caller + bounded-shutdown test; supervision that can't be
    starved; opt-in metrics TS_DEBUG_PIPELINE_METRICS; harness runner/ab-matrix2.sh). Drop or park the 2-thread
    topology unless you argue it is a useful stepping stone toward sharding.
 3. A load generator that can saturate the node without the co-located Go client (which needs 5+ cores
    for 1.7 Gbit/s): e.g. an in-process or separate-process synthetic WireGuard peer (Rust, using ts_tunnel)
    that handshakes with the node over UDP and drives M concurrent TCP flows through it (or raw IP packets to
    the forwarder), reporting throughput and the node's CPU — so we can plot throughput/CPU vs shard count
    1/2/4/8. Scripts/binaries only; I run anything live.
 Then stop and report; we'll agree on the design before implementing sharding itself.
Same rules: DCO line, own worktree, CARGO_TARGET_DIR=/Volumes/git/ts-exit/scratch/codex-target, don't touch
the live node, the runner checkout, or the Go client.
