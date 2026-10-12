#!/usr/bin/env python3
"""Plain TCP benchmark sink for a lab/VPS. d=stream zeros, u=consume EOF, e=echo.
Default bind is loopback; specify a public interface for an exit-egress measurement.
"""
import argparse
import socket
import threading
import struct

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--bind", default="127.0.0.1")
p.add_argument("--port", type=int, default=9000)
args = p.parse_args()
listener = socket.socket()
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind((args.bind, args.port))
listener.listen(128)

def serve(sock):
    with sock:
        try:
            mode = sock.recv(1)
            if mode == b"D":
                header = bytearray()
                while len(header) < 8:
                    data = sock.recv(8-len(header))
                    if not data:
                        return
                    header.extend(data)
                remaining = struct.unpack(">Q", header)[0]
                buf = bytes(64 << 10)
                while remaining:
                    n = min(remaining, len(buf))
                    sock.sendall(memoryview(buf)[:n])
                    remaining -= n
                return
            if mode == b"d":
                buf = bytes(64 << 10)
                while True:
                    sock.sendall(buf)
            elif mode in (b"u", b"e"):
                while True:
                    buf = sock.recv(64 << 10)
                    if not buf:
                        return
                    if mode == b"e":
                        sock.sendall(buf)
        except OSError:
            pass

while True:
    connection, _ = listener.accept()
    threading.Thread(target=serve, args=(connection,), daemon=True).start()
