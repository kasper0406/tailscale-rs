"""Download through a SOCKS5 proxy from the sink's 'D' mode, printing MB received per interval.
usage: socks-progress.py <proxy host:port> <dst ip:port> <seconds> [interval=0.5]"""
import socket, struct, sys, time
proxy, dst, secs = sys.argv[1], sys.argv[2], float(sys.argv[3])
iv = float(sys.argv[4]) if len(sys.argv) > 4 else 0.5
ph, pp = proxy.rsplit(":", 1); dh, dp = dst.rsplit(":", 1)
s = socket.create_connection((ph, int(pp)))
s.sendall(b"\x05\x01\x00"); assert s.recv(2) == b"\x05\x00"
s.sendall(b"\x05\x01\x00\x01" + socket.inet_aton(dh) + struct.pack(">H", int(dp))); assert s.recv(10)[1] == 0
s.sendall(b"D" + struct.pack(">Q", 1 << 40)); s.settimeout(iv)
t0 = time.monotonic(); nxt = t0 + iv; got = 0; worst = 0
while time.monotonic() - t0 < secs:
    try: got += len(s.recv(1 << 20))
    except socket.timeout: pass
    if time.monotonic() >= nxt:
        print(f"{time.strftime('%T')} {time.monotonic()-t0:5.1f}s {got/1e6:7.1f} MB", flush=True); worst = got if got else worst; got = 0; nxt += iv
