# Commit message rewrite: layered branches

Rewrote 36 local feature commits with `git commit-tree`, preserving every tree, author, committer and timestamp. All 36 subjects are unchanged. Bodies were derived from the 97 archived source commits grouped by the previous mapping; split sources contribute to each applicable feature.

The requested layer names refer to the existing `ts-exit/*` branches. Rewrote `ts-exit/fixes`, `ts-exit/exit-node`, `ts-exit/perf`, `ts-exit/main`, `ts-exit/bench` and `ts-exit/experiments` in one atomic ref transaction. The separate upstream `main` branch remains at `719aa8891fd777daafab67700d3d05894e49b71f`.

No push, fetch, remote/configuration changes, tag changes or code changes were made. All other refs, the worktree status and index entries are unchanged. The checked-out branch remains `ts-exit/main`.

Each message retains `Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>`. AI co-authors are the union of trailers on its mapped source commits. Legacy `Codex <noreply@openai.com>` trailers are normalized to the requested `GPT-6.1 (codex) <noreply@openai.com>` identity and deduplicated. No AI trailer is added where the sources had none. Message prose contains no workflow references or IP address literals.

## Branch heads and stacking

Base: `origin/main` = `3951752dc1aea1a126fcb3d271eceb36b1cdb48e` (unchanged).

| Local branch | Before | After | Layer commits | Total above base |
|---|---|---|---:|---:|
| `ts-exit/fixes` | `a57c0ca4c2a87d1351fe8bb1e2ce37f2bb0f173d` | `3daacde398e77dabc18ef5c9474172c999b3bbaa` | 13 | 13 |
| `ts-exit/exit-node` | `26cfad86c0a65327923371d3825c93a72e1b1740` | `7e7aca8f8fe31a63eec9d38b4df36275ec881cfc` | 5 | 18 |
| `ts-exit/perf` | `e909d0bb4b9dc9fc4f16fcccdbaecae9c7bf173a` | `06850529c6df1c94869ecb6342b515d11597e6f0` | 12 | 30 |
| `ts-exit/main` | `e909d0bb4b9dc9fc4f16fcccdbaecae9c7bf173a` | `06850529c6df1c94869ecb6342b515d11597e6f0` | 0 | 30 |
| `ts-exit/bench` | `a9cf235f412dba4028e4ef1abd86395312872ab0` | `e3a8385d86b09c46c487c87ee3e278cbd88f74fa` | 4 | 34 |
| `ts-exit/experiments` | `4cbe07a3debe22f612a201479543c07f2f0b1840` | `834cbfa4155b5fac2272f80c87b72c47d910cfef` | 2 | 36 |

**PASS:** Layer ancestry and counts are 13 → 5 → 12 → 4 → 2; `ts-exit/main` equals `ts-exit/perf`. Every rewritten parent is the corresponding rewritten predecessor; the first parent remains the unchanged base.

## Old → new commits and tree equality

**PASS: 36/36 trees are exactly equal.** For every row, ran `git rev-parse <old>^{tree}` and `git rev-parse <new>^{tree}` and compared the results. Both commands returned the shared tree SHA recorded below. Raw commit comparison also confirmed that only the parent and message changed; author/committer identities and dates are preserved.

Code tests were not rerun: every commit tree is identical. Validation covered all tree pairs, raw commit metadata, subjects, source-derived trailers, linear ancestry, branch counts, ref isolation, index, worktree and local configuration.

| # | Before | After | Tree SHA returned by both commands | Result |
|---:|---|---|---|---|
| 1 | `3c073136ff3b7a8cc523fc0bd348e9c43fb51138` | `d7f57dcd2f0d749d1c59d4eea356e5393a033906` | `30a30f4eec9e50174a224651cee9ee84cde1d453` | PASS |
| 2 | `16e3304be76fae27734ddffd4d6f7c0ab8bb081f` | `0bdbcd4872743fe32c1e128db84770c25dc6a1d7` | `2b954db60a721e86767b569bf6cf418ac0dfc858` | PASS |
| 3 | `87bca4de3ff1281a99ae12dace8bc065b57ee624` | `14dd94b71e1cce257c3ae18e2e6cc1e2bc7dd176` | `e9860ecb2a4cb8c9184add7f7e414ee6b9b4eabf` | PASS |
| 4 | `10c9b13ee3ad1221bb070078799d85ac7ff62d2b` | `c16080c3d035fc0b30f9a068a6e9a97f6999fd33` | `392d92b56b7b883d07f93b52a2c54b8017d3dcde` | PASS |
| 5 | `4574e3badb34eaced7758e8ddd039b24efdb1a1a` | `bf808b0232d415be357887c71cccfddfa1a778e5` | `0042010bf564cba9672d852f62b7e2c5001e9a90` | PASS |
| 6 | `b1604483d681fcb5e7bb18d2b8f3089bbad848ac` | `edf1310b3b8978dd27b32166382266fa471ddc60` | `b510686739f6b479810accc62d628693ff5e2a95` | PASS |
| 7 | `4d7e894728d3ffa70bcd1c429301f4b6fc8c518c` | `3727ebb43da0143423eada3be59f46d8d3e975a8` | `2280d1b124f6d6623ee2a79db6d46fac9a509bb1` | PASS |
| 8 | `19f8808d05241781831f55ee1dd18ae00e86b32d` | `1b9d597b57e34926aa401b261786cec4d5f20672` | `739d8801e4d2487d0f8bacad2bff663f78746c3b` | PASS |
| 9 | `2e04ae17a18155520f2f90f1a7c18454d2bacdbd` | `35e232e77f9fedbbd0b1946786fa504d1272f080` | `c8ebce54629c7511ebc4c8127ab52ba2e1c8ca1e` | PASS |
| 10 | `9634ee19f81477ffb34829b40acc8c038a99376d` | `8a7ad8e209c26417959722d90aa772f5bc21d8db` | `16cfda3c977d8175560b46ef63c2b65fa7ec6e02` | PASS |
| 11 | `5308556e899d512a6aa31cd15e5e13dfdd87284e` | `ef63e464082a5edc1888fb01d2564a67726e5a39` | `5f07d6ff1d43d240dba6f0b79e593113d812dd65` | PASS |
| 12 | `60ab7a892baa2f4268bf8e8f643b0484708ef5f6` | `0a78343a2345d4f0b2f64576e9a6dc2b621aca00` | `a91c74bcba1fbfb245dfe2cc1dccf4deb81beeef` | PASS |
| 13 | `a57c0ca4c2a87d1351fe8bb1e2ce37f2bb0f173d` | `3daacde398e77dabc18ef5c9474172c999b3bbaa` | `b8ff3a54669bcdffeb2d7db7d4806451fc5996da` | PASS |
| 14 | `3345d74b6abaad6aee054ea2da5bb847819a4e10` | `ccf32b551d4b5c3cba7522a4f7047f04080ff7a4` | `40882efc6efb61856173f0bda9c2c3166bf90815` | PASS |
| 15 | `0cf0cbc4f7802347979822007c103f0840204f2d` | `35da1fad23cba25f689729442b64a47b4090c2e8` | `863f0b91a180fa56ff4ff6fd6c306e8e2577ae84` | PASS |
| 16 | `307279b8419c13cb26c1545a9a615d948402866c` | `8a4a0faf85926a86aa32c1e997785ba069a4e9b7` | `4647c0fc68141d4a472d930e1e7c9f47a07c4608` | PASS |
| 17 | `7dc3815c78e79534d50c5842b8b1af701c485725` | `939a127c911abeb250320c567b5cbf9cc99da285` | `5bcc3a0eca11395739583e0c6a23001463e80a34` | PASS |
| 18 | `26cfad86c0a65327923371d3825c93a72e1b1740` | `7e7aca8f8fe31a63eec9d38b4df36275ec881cfc` | `31bc667985335200939691cbd210f6f90a6f4cf4` | PASS |
| 19 | `df5b9448876ac08d9551a320b26941d281917b22` | `fa14e2c5b4e22339cf5b6d2a87f8e3f3f4db888a` | `27ecdaa87adf353ce4b1210902cede32b1538888` | PASS |
| 20 | `23c898b127631570c09fbf3a7fa9990c90fc503f` | `1e70b8470e4f5de4c4ba95285ef311fd2c44e9be` | `23e10c0a9085bda4e1ed9b3dd0e11ef5c84a78d1` | PASS |
| 21 | `d8db8d1da8d04d1f1f36e4b2d9215ddb15e0fd5e` | `0928a316b5c79bd1279b7ca85bf9dccfc1349d6d` | `dbd0af5deecea9ed2d616e486d62684b9f632cbd` | PASS |
| 22 | `c26cecd59749ea3dbd3f6a73501ef04965c4f714` | `1d43127e8377ade3e90d52e823a96b578a557b72` | `7c16bf008183fcf285ec8b97756a2eecd95dd57b` | PASS |
| 23 | `435e29a84e9f60830d10b80451f7d927dad6f6b5` | `6cdca84e71c524801f5380670367dbdf478baed3` | `afaf4092e1bdbfd62379cd6b945a8ecf85b2e1d3` | PASS |
| 24 | `124d94920012c19f4d61839cb3915bc4f8b50215` | `fa1f6f34dccbe9e0c5371318da6bd8480bfb1809` | `98326728c4447555e82618b60998cf846dcbb8b6` | PASS |
| 25 | `341e6af15e30986513893cd051213ac01a65b1eb` | `e0bf5018d58fb985e95b35c4bc9ab8b780d39dc7` | `a581a73233edc6155eac2f6d9db8a741a702a52e` | PASS |
| 26 | `8c7986bae2ed24377bc9a1910df4d12694d3ae08` | `b9819c45d2a7e6055e044bba1cade88cea182452` | `ec0cb943ad3969c07ca4c7bfa948f8d2e7aa2327` | PASS |
| 27 | `df411dc1905bfb10fbefe064d65269315859452e` | `1fcd56c32a02b4b94b351ac37ea838566b89f8cb` | `4f753e3857029fc76a189c6386ee38df93b03b80` | PASS |
| 28 | `023676a3c50feaf8001f7037b6d92c032981ad34` | `a05064b89bcc218a3d56b1b0baa4e3473a6669ce` | `7af3ac84990aa935f930b5e3d2dd3d509bb43469` | PASS |
| 29 | `087d9cb38ac6466d30a5c24d40ba71ef21b0f468` | `bdf242cc468ebb8e5e35ec93af9e69eb105e07d8` | `837870bc8d1f69594219ac7f5494d37d568db507` | PASS |
| 30 | `e909d0bb4b9dc9fc4f16fcccdbaecae9c7bf173a` | `06850529c6df1c94869ecb6342b515d11597e6f0` | `e9ae71edd71aeb73cc2416d573f6f725feca7a3f` | PASS |
| 31 | `1a98c935b1211e6fab8028f45194a884d3cb0b2e` | `4993ebb882d29f53cf95dd0eefa9d486795b5e38` | `3a522b8f4453fa42356f605bad67ea829c72e307` | PASS |
| 32 | `f5b2be8a63825d164ece2b39f0fcc726b60f4522` | `f15b775414339be3634f620b8c9d134fe3d0c31b` | `d4050d995fec80992d2b5d8f5e000df36c628cd4` | PASS |
| 33 | `1c1d55f18d4c5dc5876b4fe7902bb080d50effa5` | `2ebf9c4c735bcae8b895077928fc5130bfcc1a32` | `8bfc4ed645a9445628101277c8d4ad30564bef21` | PASS |
| 34 | `a9cf235f412dba4028e4ef1abd86395312872ab0` | `e3a8385d86b09c46c487c87ee3e278cbd88f74fa` | `ac8552475f840bbb946bd428ac76a4162a483d72` | PASS |
| 35 | `b26c863cd85717b51108a46857581d8f0dad3bf5` | `4bfd2a2509c5cae270bf7c152a2ec6fdaff8652d` | `61a960ac131731fa89923982acd17839fbc34e01` | PASS |
| 36 | `4cbe07a3debe22f612a201479543c07f2f0b1840` | `834cbfa4155b5fac2272f80c87b72c47d910cfef` | `7d9abbc520cba03efb4a5845604b39cb004fb737` | PASS |

## Full new commit messages

### 1. fixes: fix(forwarder): confine host exit flows to internet destinations

Before: `3c073136ff3b7a8cc523fc0bd348e9c43fb51138`  
After: `d7f57dcd2f0d749d1c59d4eea356e5393a033906`

Archived sources: `824f2d2765607a63af9a024cac83bcec008aaaf6`.

```text
fix(forwarder): confine host exit flows to internet destinations

The default exit route allowed HostExitDialer to reach the host's LAN,
loopback services and other tailnet nodes. Reject forbidden exit
destinations before opening TCP or UDP sockets, using the proxy dialer's
existing guard. Advertised subnet flows retain access to their ranges.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 2. fixes: fix(netstack): make waking pipe try_recv non-blocking

Before: `16e3304be76fae27734ddffd4d6f7c0ab8bb081f`  
After: `0bdbcd4872743fe32c1e128db84770c25dc6a1d7`

Archived sources: `99d44744b6aae44f5eff2ee506674655fb38b20e`.

```text
fix(netstack): make waking pipe try_recv non-blocking

WakingPipeReceiver::try_recv called blocking recv and could park its
caller indefinitely on an empty pipe. Use the non-blocking flume operation
so tests and embedders can poll without waiting for a packet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 3. fixes: fix(tunnel): revoke sessions after static key or PSK changes

Before: `87bca4de3ff1281a99ae12dace8bc065b57ee624`  
After: `14dd94b71e1cce257c3ae18e2e6cc1e2bc7dd176`

Archived sources: `83f6076dc34657451baa93f81d06280ee1b61e0f`.

```text
fix(tunnel): revoke sessions after static key or PSK changes

Static-key or PSK updates left old transport keys and handshakes usable.
Recreate the peer on identity changes, freeing session IDs, cancelling
timers and discarding staged packets. Validate collisions before changing
indexes; interval-only updates keep the existing session.

Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 4. fixes: fix(runtime): remove expired peers from tunnel and routes

Before: `10c9b13ee3ad1221bb070078799d85ac7ff62d2b`  
After: `c16080c3d035fc0b30f9a068a6e9a97f6999fd33`

Archived sources: `3aba14b0b185fe619ed9926f8e579daf21d94480`.

```text
fix(runtime): remove expired peers from tunnel and routes

Expired peers retained tunnel state and usable routes. Remove their
sessions and exclude host, subnet and selected-exit routes from routing
and source validation. Keep expired nodes visible in status and recreate
the peer when control sends an unexpired update.

Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 5. fixes: fix(runtime): answer peer queries on self-only netmaps

Before: `4574e3badb34eaced7758e8ddd039b24efdb1a1a`  
After: `bf808b0232d415be357887c71cccfddfa1a778e5`

Archived sources: `eb186e772681ee9dbafa46eca2335edaedae61b0`.

```text
fix(runtime): answer peer queries on self-only netmaps

Control omits an empty peer list, leaving status and peer lookups waiting
forever on nodes that can see no peers. Treat a response carrying the
self node as the initial state, answer parked requests and publish status
with an empty peer set.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 6. fixes: fix(control): send complete Hostinfo on every map request

Before: `b1604483d681fcb5e7bb18d2b8f3089bbad848ac`  
After: `edf1310b3b8978dd27b32166382266fa471ddc60`

Archived sources: `5c35974eb0d169896fa6b523e0eacb36f358f620`.

```text
fix(control): send complete Hostinfo on every map request

Partial non-streaming map requests replaced Hostinfo wholesale, erasing
services, client version and tags; streaming Hostinfo is ignored by control.
Build complete Hostinfo for every request and send a non-streaming update
after map polling starts so peers can discover peerAPI services.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 7. fixes: fix(runtime): retain DNS config and self node across deltas

Before: `4d7e894728d3ffa70bcd1c429301f4b6fc8c518c`  
After: `3727ebb43da0143423eada3be59f46d8d3e975a8`

Archived sources: `5d6b650fec34b186d20ce400d42abebbcf999f50`.

```text
fix(runtime): retain DNS config and self node across deltas

Netmap deltas without DNSConfig or SelfNode erased the DNS actor's view,
disabling resolution and causing peerAPI Host checks to reject requests.
Retain both values across deltas and replace each only when control
includes a new value.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 8. fixes: fix(runtime): order security updates and supervise forwarding consumers

Before: `19f8808d05241781831f55ee1dd18ae00e86b32d`  
After: `1b9d597b57e34926aa401b261786cec4d5f20672`

Archived sources: `63c1e9839edbced04430740016601a6e3590d3dc`.

```text
fix(runtime): order security updates and supervise forwarding consumers

Security updates could race registration or be lost under saturation.
Deliver them serially through an unbounded broker and await subscriber
registration before control publishes maps. Independently supervise
critical actors and abort packet forwarding on death; coherent snapshots
and progress deadlines remain follow-up work.

Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 9. fixes: fix(netstack): vendor smoltcp fast-retransmit RTO recovery

Before: `2e04ae17a18155520f2f90f1a7c18454d2bacdbd`  
After: `35e232e77f9fedbbd0b1946786fa504d1272f080`

Archived sources: `acca2d95385fafd5a5623c70f00837bc9c7414b1`, `a2327e3f83090e30d57b433a8239ecd31d55ef92`, `b9972f709da09227ca2715bf22d94c120ca0dfd4`.

```text
fix(netstack): vendor smoltcp fast-retransmit RTO recovery

Fast retransmits incorrectly backed off RTO, eventually stalling lossy
TCP downloads. Vendor smoltcp 0.13.1 and backport upstream recovery fix
29340b71ffca86f130746c7dd3c7a9a09495dda9: invalidate RTT samples on fast
recovery, but back off only on timer expiry. Preserve features, no_std
and licensing, and suppress the vendor's unused-code warnings.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 10. fixes: fix(magicsock): match Go betterAddr path scoring

Before: `9634ee19f81477ffb34829b40acc8c038a99376d`  
After: `8a7ad8e209c26417959722d90aa772f5bc21d8db`

Archived sources: `0ecaacaf7b763be8eb64b414fa3fbc586c5a655e`.

```text
fix(magicsock): match Go betterAddr path scoring

Latency-only path selection could prefer a NAT hairpin over a nearby LAN
endpoint. Port Go's betterAddr address-family and locality scoring with
its hysteresis, excluding MTU/Geneve scoring. A rival's pong must not
extend the current best path's trust window.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 11. fixes: fix(netstack): preserve data after abandoned receives

Before: `5308556e899d512a6aa31cd15e5e13dfdd87284e`  
After: `ef63e464082a5edc1888fb01d2564a67726e5a39`

Archived sources: `13d4c4c1c996d49610cf0636519c1cb4a52b538d`.

```text
fix(netstack): preserve data after abandoned receives

A queued receive whose future was dropped still consumed bytes or a
datagram and sent them to a closed response channel. Discard abandoned
blocked receives during replay so the caller's retry can receive the data.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 12. fixes: fix(netstack): guarantee FIN and Close delivery by default

Before: `60ab7a892baa2f4268bf8e8f643b0484708ef5f6`  
After: `0a78343a2345d4f0b2f64576e9a6dc2b621aca00`

Archived sources: `4e6490da7c4b1510883c2e2a813b83e8bf2a9488`.

```text
fix(netstack): guarantee FIN and Close delivery by default

A full command channel silently discarded fire-and-forget ShutdownWrite
and Close commands, losing FINs and leaving flows waiting for timeouts.
Use an unbounded command channel by default; outstanding socket operations
limit in-flight requests. Explicitly bounded channels still drop on full
and now log the loss.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 13. fixes: fix(netstack): remove reset half-open listener sockets

Before: `a57c0ca4c2a87d1351fe8bb1e2ce37f2bb0f173d`  
After: `3daacde398e77dabc18ef5c9474172c999b3bbaa`

Archived sources: `12f705c30616bc5608163ff413671b9a3ad56770`.

```text
fix(netstack): remove reset half-open listener sockets

A reset SYN-RECEIVED socket returned to LISTEN and became an untracked
listener, accepting later handshakes that no Accept could return. Remove
these sockets from the socket set in the listener pump and Accept before
another SYN can reach them.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 14. exit-node: feat(forwarder): gate all-port ingress with owned generations and UDP keepalive

Before: `3345d74b6abaad6aee054ea2da5bb847819a4e10`  
After: `ccf32b551d4b5c3cba7522a4f7047f04080ff7a4`

Archived sources: `417dd2959fbf0d07deb0319d9a3a73cd1040bf17`, `af993f68cb4bb060382b009b64b7010a2fd85e46`, `9d21497e25a856ad7312d8f88f6b27d19e9b5289`, `acb0f0679b760dff82df042325af9db6d049d582`, `a55d5e225bfd99b0e65f137e601e201afa9af72d`.

```text
feat(forwarder): gate all-port ingress with owned generations and UDP keepalive

Raw all-port observation lost the first packet before creating a listener.
Hold initiating packets behind a bounded PortGate and release them once
the listener is ready. Own listeners and validate generations across
teardown and reopen. Keep UDP flows alive on replies, with two-minute
idle expiry (30 seconds for DNS), and retain ports with live flows.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 15. exit-node: feat(runtime): reject IPv6 forwarding with bounded responses

Before: `0cf0cbc4f7802347979822007c103f0840204f2d`  
After: `35da1fad23cba25f689729442b64a47b4090c2e8`

Archived sources: `8b1d19ac3f8ddaeb8e14685e249fd3acbad6eb36`, `4f825964c5c861ee49460e457a818dd71eb263f7`, `d89780d41fc21ed0c9f04a565bb52f6bc2d081d5`.

```text
feat(runtime): reject IPv6 forwarding with bounded responses

The IPv4-only forwarder could accept IPv6 handshakes and fail only when
dialing. Consume IPv6 before netstack admission and return validated TCP
resets or ICMPv6 prohibited replies where safe. Cap replies at 1000/s
with a 1000-packet burst and ICMPv6 quoting at the minimum IPv6 MTU;
unsafe or over-budget packets are dropped.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 16. exit-node: feat(control): advertise both default routes for exit nodes

Before: `307279b8419c13cb26c1545a9a615d948402866c`  
After: `8a4a0faf85926a86aa32c1e997785ba069a4e9b7`

Archived sources: `c49d9468b53eff439a14548df9d6422e31a20eec`.

```text
feat(control): advertise both default routes for exit nodes

Official clients require both IPv4 and IPv6 default routes before
offering a peer as an exit node. Advertise the pair at startup and on
live configuration changes. IPv6 forwarding remains unsupported and
receives prompt rejection so dual-stack clients can fall back to IPv4.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 17. exit-node: feat(runtime): resolve exit-node DNS through host resolver and peerAPI

Before: `7dc3815c78e79534d50c5842b8b1af701c485725`  
After: `939a127c911abeb250320c567b5cbf9cc99da285`

Archived sources: `771ad65551ead47ba554c947ab6786fc20877209`, `3aa177299c0f1de47bd507e47133f9662a271e3a`.

```text
feat(runtime): resolve exit-node DNS through host resolver and peerAPI

Exit clients could not reach their private DNS resolvers or discover a
working peerAPI DNS service. Expose Config::peerapi_port and use
Config::exit_node_dns for host-socket resolution, with TCP retry on UDP
truncation. Redirect only forbidden exit destinations on port 53; retain
peer authorization, egress gates and tailnet-name anti-leak checks.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 18. exit-node: feat(examples): add interactive exit node with DNS and four workers

Before: `26cfad86c0a65327923371d3825c93a72e1b1740`  
After: `7e7aca8f8fe31a63eec9d38b4df36275ec881cfc`

Archived sources: `40902939812dc7878cf1534ac34ef144639b8177`, `6dc79b908387655685f46bad7d7f7b78a0fe2b41`, `5ea31902ee2da5b20ff92b472b7c31e2d86c2bc1`, `9cc89643c80d0ee503754c7eec9fdf540102c0ea`, `29a588e9def3fcb4f9a7ef5875922f963ec78db9`, `28684afb3f5fbb488d45f9e70cc0b0f39c546554`, `d8250d84b7e5f7ffadbec62dfa1e7d32332453c9`, `0db91e620d68d6287e8b464f7c6ca9d984b75871`, `5cf4baf463ceab65ca6c925155f1b11d373b9637`, `e24457d59c8fadebd913bacba90d2d599ea271f0`, `c4dffbed958e042a0e46125f95d143c3df309ffb`.

```text
feat(examples): add interactive exit node with DNS and four workers

Add a userspace exit-node runner with persisted identity, auth-key-file
or --interactive login, and bounded periodic status. Serve host-resolver
DNS and peerAPI on port 41643 by default; --no-exit-dns and
--peerapi-port 0 disable them. Default --worker-threads to four to avoid
parallel-flow starvation, and provide a --sink-port throughput/echo sink
without changing host routes, DNS or packet filters.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 19. perf: perf: add an optimized exitnode Cargo profile

Before: `df5b9448876ac08d9551a320b26941d281917b22`  
After: `fa14e2c5b4e22339cf5b6d2a87f8e3f3f4db888a`

Archived sources: `30d551a12f720e3f4f81d0da7586f0546e0f30ce`.

```text
perf: add an optimized exitnode Cargo profile

The release profile optimizes for size and strips symbols, limiting
throughput and profiling. Add an exitnode profile with opt-level 3 and
line tables: cargo build --profile exitnode --example exit_node.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 20. perf: perf: keep packet-path spans at trace level

Before: `23c898b127631570c09fbf3a7fa9990c90fc503f`  
After: `1e70b8470e4f5de4c4ba95285ef311fd2c44e9be`

Archived sources: `80e78b9e6fbb4605c4a90edb14f6f69916bc16eb`.

```text
perf: keep packet-path spans at trace level

Default INFO instrumentation created and formatted spans for every
packet, command and poll under ordinary logging. Move packet-path spans
to TRACE to avoid that overhead while retaining detailed diagnostics
when requested.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 21. perf: perf(runtime): batch netstack egress without copying

Before: `d8db8d1da8d04d1f1f36e4b2d9215ddb15e0fd5e`  
After: `0928a316b5c79bd1279b7ca85bf9dccfc1349d6d`

Archived sources: `3a5f0d771b614934563f6841e712af0766a0d177`.

```text
perf(runtime): batch netstack egress without copying

Application and forwarder egress pumps copied each packet into a
one-element batch. Drain up to 64 already queued packets without waiting
and convert uniquely owned Bytes into PacketMut without copying, allowing
the dataplane and underlay to process batches.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 22. perf: perf(magicsock): provision seven MiB UDP socket buffers

Before: `c26cecd59749ea3dbd3f6a73501ef04965c4f714`  
After: `1d43127e8377ade3e90d52e823a96b578a557b72`

Archived sources: `6e21f8a8710c6e12a62081da857a47201c1ef772`.

```text
perf(magicsock): provision seven MiB UDP socket buffers

Small OS-default UDP buffers dropped bursts with ENOBUFS and forced
TCP retransmission inside the tunnel, especially on macOS. Request
seven MiB send and receive buffers on bind and rebind, matching Go
magicsock's provisioning.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 23. perf: perf(tunnel): add opt-in ring AEAD with transport benchmarks

Before: `435e29a84e9f60830d10b80451f7d927dad6f6b5`  
After: `6cdca84e71c524801f5380670367dbdf478baed3`

Archived sources: `3196e100b7bab3e91b0b3f547ba2369fa55fbd1d`, `378fab5b668eaec10d49da58c57b732104d7a0d5`.

```text
perf(tunnel): add opt-in ring AEAD with transport benchmarks

Add the opt-in ring-aead Cargo feature for in-place WireGuard transport
seal/open, with production-session and tag-verification benchmarks.
Preserve nonce, padding, error and replay ordering, and wipe failed opens.
RustCrypto remains the default; raw keys are zeroized, but ring's internal
key erasure is not guaranteed. Timing tests do not prove constant-time behavior.

Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 24. perf: perf(magicsock): add connected Apple UDP paths and sendmsg_x batches

Before: `124d94920012c19f4d61839cb3915bc4f8b50215`  
After: `fa1f6f34dccbe9e0c5371318da6bd8480bfb1809`

Archived sources: `d918b16629346f7584e1aa594779c8d3aa7e5777`, `bfb17c1e89bccafb60c3a32084c8f4183943ff67`.

```text
perf(magicsock): add connected Apple UDP paths and sendmsg_x batches

Apple send_to incurs per-packet routing overhead. Promote busy direct
paths to connected sockets on the same local port, capped at 256 with
idle eviction and rebind cleanup; retain shared-socket fallback. Opt in
with TS_DEBUG_MAGICSOCK_CONNECTED_SOCKETS=1, optionally
TS_DEBUG_DONT_FRAGMENT=1. TS_DEBUG_MAGICSOCK_BATCHED_IO=1 enables
sendmsg_x batches of up to 64, with per-packet fallback on failure.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 25. perf: perf(netstack): validate socket generations and bound orphan sweeps

Before: `341e6af15e30986513893cd051213ac01a65b1eb`  
After: `e0bf5018d58fb985e95b35c4bc9ab8b780d39dc7`

Archived sources: `664ae33edabf8a61aa548e10e4226b7a25ab502a`, `c2b3dbe0531e7a96e37a30aec1145ad6ebeafab6`.

```text
perf(netstack): validate socket generations and bound orphan sweeps

Repeated socket scans wasted work and recycled slots could inherit
stale closes. Validate typed access through the generation registry and
deduplicate pending closes, clearing them on removal. Sweep live entries
at most every 100 ms, including idle periods; track pending connects
separately for prompt reset cleanup. TS_DEBUG_PIPELINE_METRICS=1 counts work.

Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 26. perf: perf(netstack): replay only socket waiters whose state changed

Before: `8c7986bae2ed24377bc9a1910df4d12694d3ae08`  
After: `b9819c45d2a7e6055e044bba1cade88cea182452`

Archived sources: `de3a6bf212be364da17746093cb73533604e7c0c`.

```text
perf(netstack): replay only socket waiters whose state changed

Global waiter replay revisited sockets whose state had not changed.
Add generation-indexed TCP/UDP FIFO queues driven by cached readiness
wakers, with cancellation cleanup and a 50 ms full-sweep safety deadline.
Opt in with TS_EXPERIMENT_TARGETED_REPLAY=1; retain failed-connect results
and expose targeted and safety-net counters.

Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 27. perf: perf(rx): batch kernel receives and bound only receive-side work

Before: `df411dc1905bfb10fbefe064d65269315859452e`  
After: `1fcd56c32a02b4b94b351ac37ea838566b89f8cb`

Archived sources: `8f6729e102b7526fd21ad4fed71c2f1ee89cb5a2`, `e92ed02271164edaf55544aec1239f5544d65053`, `b20e50bcde10315c18c82cf23c541774b430412b`, `9251a6bea082d7af0a3a02d21d57e75cb74070cd`.

```text
perf(rx): batch kernel receives and bound only receive-side work

Per-packet receive handoffs added syscall and wake overhead. Enable
bounded kernel and per-peer batches with TS_EXPERIMENT_RX_BATCH=1, using
recvmsg_x on Apple and recvmmsg on Linux with ordinary receive fallback.
Preserve FIFO tails across cancellation and defer ingress wakes to batch
end. Bound receive work and yield on exhaustion while commands and egress
retain their normal drains.

Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 28. perf: perf(rx): reuse receive storage and remove redundant copies

Before: `023676a3c50feaf8001f7037b6d92c032981ad34`  
After: `a05064b89bcc218a3d56b1b0baa4e3473a6669ce`

Archived sources: `7d336bba242290d565d8953278d40e82724a9636`.

```text
perf(rx): reuse receive storage and remove redundant copies

Receive processing repeatedly allocated storage and copied owned
packets. Reuse receive storage and transfer packet ownership through
direct and relay demux; avoid egress iterator allocations and capacity
locking on unbounded pipe transmits. Keep receive spans at TRACE and
preserve the sender TxToken API and ordered delivery before control errors.

Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 29. perf: perf(forwarder): coalesce ready TCP bytes and splice host writes

Before: `087d9cb38ac6466d30a5c24d40ba71ef21b0f468`  
After: `bdf242cc468ebb8e5e35ec93af9e69eb105e07d8`

Archived sources: `0698abee266705b9677c1a959a49f57acb170c97`, `6c08daa5fc918e8ae950362fae28c9f46bcaf64b`.

```text
perf(forwarder): coalesce ready TCP bytes and splice host writes

TCP ring wrap and small reads fragmented host writes. Add bounded
ready-byte receives and opt-in 128 KiB splice buffers through
TS_EXPERIMENT_SPLICE_COALESCE=1, gathering only bytes already queued.
Preserve abandoned-receive protection, backpressure and independent
half-close handling; record host-write sizes and partial completions
when metrics are enabled.

Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 30. perf: perf: keep Apple batching defaults and bound Linux handoffs and bursts

Before: `e909d0bb4b9dc9fc4f16fcccdbaecae9c7bf173a`  
After: `06850529c6df1c94869ecb6342b515d11597e6f0`

Archived sources: `e59e731bf5068d4ee5f392d1657071b10be69638`, `3e9ee27fb19e73b91b4e852c6bf6df4320b09117`, `f01ba7c63e360f4043e1a31c2dbb211f338fb2b1`, `39a62208c20bafcf62f5c2476e74759a6a67b7c3`.

```text
perf: keep Apple batching defaults and bound Linux handoffs and bursts

Linux measurements showed batching regressions, while Apple benefited.
Default targeted replay on everywhere and RX batching/splice coalescing
on only on Apple; TS_EXPERIMENT_TARGETED_REPLAY, TS_EXPERIMENT_RX_BATCH
and TS_EXPERIMENT_SPLICE_COALESCE accept 0/1 overrides. On enabled Linux
paths, retain recvmmsg but hand off at most four packets per turn and
cap host-to-overlay copies at 16 KiB without speculative ready-read drains.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 31. bench: bench(forwarder): add finite throughput, port-mode and loss workloads

Before: `1a98c935b1211e6fab8028f45194a884d3cb0b2e`  
After: `4993ebb882d29f53cf95dd0eefa9d486795b5e38`

Archived sources: `4d5f234de9fe015f25d96db0a107f257f50eefe5`, `50cce10cc8fa9de038a80991de54822eaf3aae46`, `91d29093713fd601269053a02be7affaa407d0d2`, `758b79aecd39c1be7e47e789a603d40118f8185e`.

```text
bench(forwarder): add finite throughput, port-mode and loss workloads

Add isolated forwarder throughput and cold-connect workloads with
RTT_MS, TCP_BUF_KB and LOSS_PCT controls. FWD_MODE compares explicit
listeners, raw observation and gated all-port ingress. Bound connection
and transfer waits, count partial writes, and use VERIFY_MB for finite
completion checks so lossy runs fail at a deadline instead of hanging.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 32. bench: bench: add bounded A/B workloads and independent latency probes

Before: `f5b2be8a63825d164ece2b39f0fcc726b60f4522`  
After: `f15b775414339be3634f620b8c9d134fe3d0c31b`

Archived sources: `3300a85af20ab17d37f35415ade0b04d6385da99`.

```text
bench: add bounded A/B workloads and independent latency probes

Add bounded A/B workloads that alternate mode order and record actual
completed bytes, per-flow times, upload EOF acknowledgements, CPU,
binary hashes and flags. Run independent loaded echo probes without
extending the throughput measurement interval.

Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 33. bench: bench(forwarder): allow explicit local sink redirects for benchmark addresses

Before: `1c1d55f18d4c5dc5876b4fe7902bb080d50effa5`  
After: `2ebf9c4c735bcae8b895077928fc5130bfcc1a32`

Archived sources: `b7216a64440e67762102c22fa664eb98d7cc5ec2`.

```text
bench(forwarder): allow explicit local sink redirects for benchmark addresses

Exit-path benchmarks need a local sink while ordinary exit confinement
rejects loopback. TS_BENCH_EXIT_REDIRECT explicitly maps one RFC 2544
benchmark-range address and port to a loopback sink. Reject other mapping
ranges and pass unmatched flows through the normal destination guard.
The redirect is disabled by default and warns when enabled.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 34. bench: bench: add authenticated synthetic peer with persistent sessions and safe teardown

Before: `a9cf235f412dba4028e4ef1abd86395312872ab0`  
After: `e3a8385d86b09c46c487c87ee3e278cbd88f74fa`

Archived sources: `4c8c372f7f3439fc99e381bab5b3e61ba7057672`, `8a2a0e1ee50db5e7541910107574765228de12e6`, `ad104685852052aba4ec9a4da8dcb6533791e7d4`, `97e49341af2a962979e2085d6ac34c24fe35abff`, `92d2c78a252a733379ababa27b6413b19701df22`, `818deb29a4ea50e96dc6bde21b84a1962340c31d`, `6819586960363ea1b125a6747fc3fa84281e8573`, `1949151671f1636a1e0ee2c1b22575a86e465a81`.

```text
bench: add authenticated synthetic peer with persistent sessions and safe teardown

Add an independent enrolled WireGuard/TCP load peer with disco preflight,
finite transfers and per-flow/CPU accounting. Support login-link enrollment
and a persistent hold session advertising the LAN endpoint so repeated
runs cannot race late offline updates. Require a hostname when advertising,
report stalled-flow progress, wait for STOP up to the transfer timeout,
and treat bounded post-measurement teardown as best effort.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 35. experiments: experiment: restore opt-in pipeline, work budgets and forwarder sharding

Before: `b26c863cd85717b51108a46857581d8f0dad3bf5`  
After: `4bfd2a2509c5cae270bf7c152a2ec6fdaff8652d`

Archived sources: `ba189ab6e67b845404f3f0b5d33dd397a3419d71`, `a31f0a0ca155c710ee1b8c5329260f2eee3b0847`, `8f6729e102b7526fd21ad4fed71c2f1ee89cb5a2`, `e92ed02271164edaf55544aec1239f5544d65053`, `dce045ff7e80b101b61702448c579f4f05b56a4a`, `fa8c9cdcf3713e7daecbea68c7b3c4c6e59ca00d`, `b7216a64440e67762102c22fa664eb98d7cc5ec2`, `1292587846e76672a8eef0844a71e478908e8160`, `3564b87d15efac472f368e02640fdf6264ead36d`, `b20e50bcde10315c18c82cf23c541774b430412b`, `7d336bba242290d565d8953278d40e82724a9636`, `9251a6bea082d7af0a3a02d21d57e75cb74070cd`, `dbb32f2c008670041945448ced5521c4164d1beb`, `fb234c6ac143d85b416a4f78b79a766e91ff5c0b`, `559762297ae0d2fe2a8402f614e7a6d8aef2f91e`, `310e9a3d3c65b85a8e70e744450200b11704317e`, `6b40ef2fd037c8b92be542477349453992ca790b`, `66db0f16ab6dde6bf1b851336fb5290fea7f7ff8`, `026886f726e051b37793c66b58500da738acfe62`, `31d6f7685f29b887bc0c76316f38b653d4c02ef8`, `a11c9f1921c5bcd38a36b1a8a7cae7f6f5e1d7d7`, `b6acf2afe91a0c4bb1666ec700d1c148e331a46e`, `41631e241c4432796184ae3759bada49eeb1f2e5`.

```text
experiment: restore opt-in pipeline, work budgets and forwarder sharding

Add opt-in packet isolation, full work budgets, ACK publication and
authenticated exit-flow shards to study executor and handoff overhead.
Use bounded admission/reassembly and serial WireGuard ownership; revalidate
queued packets at security barriers and fail closed on actor death or
source reassignment. Controls: TS_EXPERIMENT_PIPELINE_THREAD=1,
TS_EXPERIMENT_NETSTACK_BUDGET=1, TS_EXPERIMENT_ACK_BATCH=1,
TS_EXPERIMENT_FORWARDER_SHARDS (2-64) and TS_EXPERIMENT_SHARD_BATCH (1 or wg).
Multiple shards require Netstack transport and cannot combine with packet
isolation. Add metrics and isolated probes; the WireGuard/RX owner stays serial.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```

### 36. experiments: docs: preserve internal experiment and benchmark reports

Before: `4cbe07a3debe22f612a201479543c07f2f0b1840`  
After: `834cbfa4155b5fac2272f80c87b72c47d910cfef`

Archived sources: `0684e380d017a15ef5bb4be6b6f0705ff7e83a98`, `ebf893d3cd8f9975a1f949f12aab5e77a9fe1403`, `27cc9cfe02fbfbf17ca84701d6377849e2214998`, `85dd5f59e6ab9165ec136d66f3cf4c19873adf78`, `0753c5cb7aeb5afbe98d7286082946b1997ebc4d`, `6ea505a31935c8a43c614661f56e30aae937d593`, `0a70e1bb77307b6ed5f471474acd0c77507544a7`, `41631e241c4432796184ae3759bada49eeb1f2e5`.

```text
docs: preserve internal experiment and benchmark reports

Record netstack readiness, batching, work-budget and sharding designs
with validation, counter semantics and benchmark results. Document
ownership and security limits, platform policies, bounded reclamation
tradeoffs and controlled A/B matrices, distinguishing measurements from
unverified performance hypotheses.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Co-Authored-By: GPT-6.1 (codex) <noreply@openai.com>
Signed-off-by: "Kasper Nielsen" <kasper0406@gmail.com>
```
