"""Benchmark client for the exit_node --sink-port service, through a SOCKS5 proxy.
usage: python3 sinkclient.py <proxy_host:port> <dst_ip:port> <d|u> <bytes>
Prints bytes moved and seconds elapsed (after the SOCKS handshake)."""
import socket, struct, sys, time

proxy, dst, mode, total = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
ph, pp = proxy.rsplit(":", 1)
dh, dp = dst.rsplit(":", 1)
s = socket.create_connection((ph, int(pp)))
s.sendall(b"\x05\x01\x00")
assert s.recv(2) == b"\x05\x00"
s.sendall(b"\x05\x01\x00\x01" + socket.inet_aton(dh) + struct.pack(">H", int(dp)))
rep = s.recv(10)
assert rep[1] == 0, f"socks connect failed: {rep!r}"
s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 << 20)
s.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4 << 20)
t0 = time.time()
s.sendall(mode.encode())
moved = 0
if mode == "d":
    buf = bytearray(1 << 20)
    view = memoryview(buf)
    while moved < total:
        n = s.recv_into(view, min(len(buf), total - moved))
        if not n:
            break
        moved += n
else:
    chunk = bytes(1 << 20)
    while moved < total:
        n = min(len(chunk), total - moved)
        s.sendall(chunk[:n])
        moved += n
    s.shutdown(socket.SHUT_WR)
    s.recv(1)
print(moved, time.time() - t0)
