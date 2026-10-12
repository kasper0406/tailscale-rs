"""Round-trip latency through the tunnel: echo 64-byte messages via the exit_node sink's `e` mode.
usage: python3 rttprobe.py <proxy_host:port> <dst_ip:port> [count=200] -> prints p50/p90/p99/max ms"""
import socket, struct, sys, time

proxy, dst = sys.argv[1], sys.argv[2]
count = int(sys.argv[3]) if len(sys.argv) > 3 else 200
ph, pp = proxy.rsplit(":", 1); dh, dp = dst.rsplit(":", 1)
s = socket.create_connection((ph, int(pp)))
s.sendall(b"\x05\x01\x00"); assert s.recv(2) == b"\x05\x00"
s.sendall(b"\x05\x01\x00\x01" + socket.inet_aton(dh) + struct.pack(">H", int(dp)))
assert s.recv(10)[1] == 0
s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
s.sendall(b"e")
msg = b"x" * 64
rtts = []
for _ in range(count):
    t = time.perf_counter(); s.sendall(msg); got = 0
    while got < len(msg): got += len(s.recv(len(msg) - got))
    rtts.append((time.perf_counter() - t) * 1000); time.sleep(0.01)
rtts.sort()
q = lambda f: rtts[min(len(rtts) - 1, int(f * len(rtts)))]
print(f"p50={q(.5):.2f} p90={q(.9):.2f} p99={q(.99):.2f} max={rtts[-1]:.2f} ms")
