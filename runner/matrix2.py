#!/usr/bin/env python3
"""Finite sink workload. Per-flow timing and probe timing are independent.
The sink protocol is exit_node --sink-port: d=stream, u=read until EOF, e=echo.
"""
import argparse
import concurrent.futures
import hashlib
import json
import math
import os
import socket
import struct
import subprocess
import threading
import time


def exact(sock, n):
    result = bytearray()
    while len(result) < n:
        chunk = sock.recv(n - len(result))
        if not chunk:
            raise EOFError("short SOCKS/echo response")
        result.extend(chunk)
    return bytes(result)


def connect(destination, proxy, timeout):
    host, port = destination.rsplit(":", 1)
    sock = socket.create_connection(tuple(proxy.rsplit(":", 1)) if proxy else (host, int(port)), timeout)
    try:
        sock.settimeout(timeout)
        if proxy:
            sock.sendall(b"\x05\x01\x00")
            if exact(sock, 2) != b"\x05\x00":
                raise ValueError("SOCKS authentication refused")
            sock.sendall(b"\x05\x01\x00\x01" + socket.inet_aton(host) + struct.pack(">H", int(port)))
            head = exact(sock, 4)
            if head[1] != 0:
                raise ValueError(f"SOCKS connect failed: {head[1]}")
            sizes = {1: 4, 4: 16}
            size = exact(sock, 1)[0] if head[3] == 3 else sizes[head[3]]
            exact(sock, size + 2)
        return sock
    except BaseException:
        sock.close()
        raise


def cpu_seconds(pid):
    value = subprocess.check_output(["/bin/ps", "-o", "time=", "-p", str(pid)], text=True).strip()
    parts = value.split(":")
    # ps uses [[dd-]hh:]mm:ss[.fraction]. Never assume hundredths versus whole seconds.
    days = 0
    if "-" in parts[0]:
        d, parts[0] = parts[0].split("-")
        days = int(d)
    return days * 86400 + sum(float(p) * 60 ** i for i, p in enumerate(reversed(parts)))


def flow(args, index, ready, go):
    moved = 0
    start = None
    try:
        with connect(args.destination, args.proxy, args.timeout) as sock:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 << 20)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4 << 20)
            ready.release()
            if not go.wait(args.timeout):
                raise TimeoutError("start barrier")
            start = time.monotonic()
            deadline = start + args.timeout
            sock.sendall(args.direction.encode())
            buf = bytearray(1 << 20)
            while moved < args.bytes:
                if time.monotonic() >= deadline:
                    raise TimeoutError("finite transfer deadline")
                sock.settimeout(max(0.001, deadline - time.monotonic()))
                n = min(len(buf), args.bytes - moved)
                if args.direction == "d":
                    n = sock.recv_into(memoryview(buf)[:n])
                    if not n:
                        raise EOFError("download ended before byte target")
                else:
                    sock.sendall(memoryview(buf)[:n])
                moved += n
            if args.direction == "u":
                sock.shutdown(socket.SHUT_WR)
                # Sink closes after draining EOF: include remote completion, not just local buffering.
                if sock.recv(1):
                    raise ValueError("unexpected upload acknowledgement")
        end = time.monotonic()
        return dict(flow=index, bytes=moved, start=start, end=end, seconds=end-start, ok=True)
    except BaseException as error:
        if start is None:
            ready.release()
        return dict(flow=index, bytes=moved, start=start, end=time.monotonic(), ok=False, error=str(error))


def probe(args, ready, go, stop, samples):
    try:
        with connect(args.destination, args.proxy, args.timeout) as sock:
            sock.sendall(b"e")
            ready.set()
            if not go.wait(args.timeout):
                return
            payload = bytes(range(64))
            while not stop.is_set():
                start = time.monotonic()
                try:
                    sock.settimeout(min(2.0, args.timeout))
                    sock.sendall(payload)
                    if exact(sock, len(payload)) != payload:
                        raise ValueError("echo payload mismatch")
                    samples.append(dict(start=start, end=time.monotonic(), ok=True))
                except BaseException as error:
                    samples.append(dict(start=start, end=time.monotonic(), ok=False, error=str(error)))
                    return  # A timed-out stream cannot safely match the next response.
                stop.wait(args.probe_interval)
    except BaseException as error:
        ready.set()
        samples.append(dict(start=time.monotonic(), end=time.monotonic(), ok=False, error=str(error)))


def run(args):
    ready, go, stop = threading.Semaphore(0), threading.Event(), threading.Event()
    samples = []
    probe_ready = threading.Event()
    probe_thread = threading.Thread(target=probe, args=(args, probe_ready, go, stop, samples))
    probe_thread.start()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.flows) as pool:
        futures = [pool.submit(flow, args, i, ready, go) for i in range(args.flows)]
        for _ in futures:
            if not ready.acquire(timeout=args.timeout):
                raise TimeoutError("flow setup")
        if not probe_ready.wait(args.timeout):
            raise TimeoutError("probe setup")
        node_cpu_start = cpu_seconds(args.node_pid) if args.node_pid else None
        client_cpu_start = time.process_time()
        start = time.monotonic()
        go.set()
        rows = [f.result() for f in futures]
        end = max(r["end"] for r in rows)
        node_cpu_end = cpu_seconds(args.node_pid) if args.node_pid else None
        client_cpu_end = time.process_time()
        stop.set()
    probe_thread.join(timeout=min(2.0, args.timeout) + 1)
    if probe_thread.is_alive():
        raise RuntimeError("probe did not stop")
    loaded = [s for s in samples if start <= s["start"] < end]
    # Latencies starting under load remain included even if they finish after the transfer.
    latencies = sorted((s["end"]-s["start"])*1000 for s in loaded if s["ok"])
    total = sum(r["bytes"] for r in rows)
    cpu = None if node_cpu_start is None else node_cpu_end-node_cpu_start
    result = dict(label=args.label, direction=args.direction, flows=rows,
        window_start=start, window_end=end, seconds=end-start, bytes=total,
        mbit_s=total*8/(end-start)/1e6, node_cpu_s=cpu,
        node_cpu_s_per_gb=None if cpu is None or not total else cpu/(total/1e9),
        generator_cpu_s=client_cpu_end-client_cpu_start,
        probes=loaded, probe_window_seconds=end-start,
        probe_p99_ms=latencies[min(len(latencies)-1, math.ceil(len(latencies)*0.99)-1)] if latencies else None,
        probe_stalls=sum(not s["ok"] or s["end"]-s["start"] > 0.2 for s in loaded),
        probe_setup_failure=not loaded,
        # ps samples bracket the transfer with small sampling overhead; raw samples retained.
        node_cpu_start=node_cpu_start, node_cpu_end=node_cpu_end,
        binary_sha256=hashlib.sha256(open(args.binary, "rb").read()).hexdigest() if args.binary else None,
        flags=args.flags, wall_timestamp=time.time())
    print(json.dumps(result, sort_keys=True), flush=True)
    return 0 if all(r["ok"] for r in rows) and loaded else 1


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--destination", required=True)
    p.add_argument("--proxy", help="SOCKS5 IPv4 proxy; omit for direct fixture connection")
    p.add_argument("--direction", choices=["d", "u"], required=True)
    p.add_argument("--bytes", type=int, default=500_000_000)
    p.add_argument("--flows", type=int, default=8)
    p.add_argument("--timeout", type=float, default=120)
    p.add_argument("--probe-interval", type=float, default=0.02)
    p.add_argument("--node-pid", type=int)
    p.add_argument("--binary")
    p.add_argument("--flags", default="")
    p.add_argument("--label", default="")
    args = p.parse_args()
    if args.bytes < 1 or args.flows < 1 or args.timeout <= 0:
        p.error("positive bytes/flows/timeout required")
    return run(args)

if __name__ == "__main__":
    raise SystemExit(main())
