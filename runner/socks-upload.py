"""N concurrent uploads through a SOCKS5 proxy to the sink's 'u' mode; each half-closes and waits
for the sink's EOF. Reports flows that never see EOF (a lost FIN somewhere on the path).
usage: socks-upload.py <proxy host:port> <dst ip:port> <flows> <bytes> [eof_timeout_s=20]"""
import socket, struct, sys, threading, time
proxy, dst, flows, size = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
eof_timeout = float(sys.argv[5]) if len(sys.argv) > 5 else 20
ph, pp = proxy.rsplit(":", 1); dh, dp = dst.rsplit(":", 1)
results = {}
def flow(i):
    try:
        s = socket.create_connection((ph, int(pp)))
        s.sendall(b"\x05\x01\x00"); assert s.recv(2) == b"\x05\x00"
        s.sendall(b"\x05\x01\x00\x01" + socket.inet_aton(dh) + struct.pack(">H", int(dp)))
        assert s.recv(10)[1] == 0
        s.sendall(b"u"); buf = bytes(64 << 10); left = size
        while left: n = min(left, len(buf)); s.sendall(buf[:n]); left -= n
        s.shutdown(socket.SHUT_WR); t = time.monotonic(); s.settimeout(eof_timeout)
        try: results[i] = ("eof", time.monotonic() - t) if s.recv(1) == b"" else ("data", 0)
        except socket.timeout: results[i] = ("NO-EOF", eof_timeout)
        s.close()
    except Exception as e: results[i] = ("error " + repr(e), 0)
t0 = time.monotonic()
ts = [threading.Thread(target=flow, args=(i,)) for i in range(flows)]
[t.start() for t in ts]; [t.join() for t in ts]
bad = {i: r for i, r in results.items() if r[0] != "eof"}
print(f"flows={flows} wall={time.monotonic()-t0:.1f}s ok={flows-len(bad)} bad={bad} max_eof_wait={max(r[1] for r in results.values() if r[0]=='eof'):.2f}s")
