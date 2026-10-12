#!/usr/bin/env python3
"""No node restart/control changes. Run independently enrolled Rust WireGuard peers.
Config JSON: [{"key_file":..., "disco_key_file":..., "node_key":public_hex,
 "node_disco_key":public_hex, "endpoint":"ip:port", "source":"100.x.y.z",
 "destination":"public_ip:sink_port"}, ...]. Distinct authorized keys/IPs required.
The externally configured node's shard count belongs in --label, never faked here.
"""
import argparse
import hashlib
import json
import selectors
import subprocess
import time
from matrix2 import cpu_seconds


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", required=True)
    p.add_argument("--peers", required=True)
    p.add_argument("--node-pid", required=True, type=int)
    p.add_argument("--node-binary", required=True)
    p.add_argument("--label", required=True, help="Record node knob/flags/shard count; does not set them")
    p.add_argument("--flows-per-peer", default=8, type=int)
    p.add_argument("--bytes", default=500_000_000, type=int)
    p.add_argument("--workers", default=2, type=int)
    p.add_argument("--runs", default=8, type=int)
    p.add_argument("--timeout", default=120, type=int)
    args = p.parse_args()
    peers = json.load(open(args.peers))
    if not peers or len({p["key_file"] for p in peers}) != len(peers) or len({p["source"] for p in peers}) != len(peers):
        p.error("distinct key files and sources required (one key's parallel handshakes replace sessions)")
    if min(args.flows_per_peer, args.bytes, args.workers, args.runs, args.timeout) <= 0:
        p.error("positive workload parameters required")
    hashes = {name: hashlib.sha256(open(path, "rb").read()).hexdigest() for name, path in
              [("generator", args.binary), ("node", args.node_binary)]}
    for run in range(args.runs):
        for direction in ("down", "up"):
            children = []
            try:
                for entry in peers:
                    command = [args.binary, "--workers", str(args.workers), "peer", "--wait-for-start"]
                    for flag in ("key_file", "node_key", "disco_key_file", "node_disco_key", "endpoint", "source", "destination"):
                        command.extend(["--"+flag.replace("_", "-"), entry[flag]])
                    command.extend(["--flows", str(args.flows_per_peer), "--bytes", str(args.bytes),
                        "--direction", direction, "--timeout", str(args.timeout)])
                    if entry.get("legacy_stream"):
                        command.append("--legacy-stream")
                    children.append(subprocess.Popen(command, stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True))
                with selectors.DefaultSelector() as selector:
                    for child in children:
                        selector.register(child.stdout, selectors.EVENT_READ)
                    deadline = time.monotonic()+args.timeout+10
                    while selector.get_map():
                        events = selector.select(max(0, deadline-time.monotonic()))
                        if not events:
                            raise TimeoutError("peer setup barrier")
                        for key, _ in events:
                            line = key.fileobj.readline()
                            if not line or not json.loads(line).get("ready"):
                                raise RuntimeError("peer failed before readiness (check keys/disco/policy)")
                            selector.unregister(key.fileobj)
                # All TCP connections/WG handshakes exist before the measured transfer window.
                cpu0 = cpu_seconds(args.node_pid)
                wall0 = time.monotonic()
                for child in children:
                    child.stdin.write("GO\n")
                    child.stdin.flush()
                deadline = time.monotonic()+args.timeout+5
                rows = []
                completions = []
                with selectors.DefaultSelector() as selector:
                    for child in children:
                        selector.register(child.stdout, selectors.EVENT_READ, child)
                    while selector.get_map():
                        events = selector.select(max(0, deadline-time.monotonic()))
                        if not events:
                            raise TimeoutError("finite transfer")
                        for key, _ in events:
                            line = key.fileobj.readline()
                            if not line:
                                raise RuntimeError("peer exited without a completed transfer")
                            rows.append(json.loads(line))
                            completions.append(time.monotonic())
                            selector.unregister(key.fileobj)
                # Do not include process teardown in throughput or node CPU.
                wall = max(completions)-wall0
                cpu = cpu_seconds(args.node_pid)-cpu0
                for child in children:
                    child.stdin.write("STOP\n")
                    child.stdin.flush()
                for child in children:
                    _, error = child.communicate(timeout=10)
                    if child.returncode:
                        raise RuntimeError("synthetic peer failed: "+error)
                moved = sum(row["bytes"] for row in rows)
                print(json.dumps(dict(label=args.label, run=run+1, direction=direction, peers=rows,
                    binary_sha256=hashes, bytes=moved, node_cpu_s=cpu, node_cpu_s_per_gb=cpu/(moved/1e9),
                    transfer_seconds=wall, mbit_s=moved*8/wall/1e6,
                    generator_cpu_s=sum(row["generator_cpu_s"] or 0 for row in rows),
                    flags={"generator_workers":args.workers,"flows_per_peer":args.flows_per_peer},
                    timestamp=time.time())), flush=True)
            finally:
                for child in children:
                    if child.poll() is None:
                        child.kill()
                    child.communicate()

if __name__ == "__main__":
    main()
