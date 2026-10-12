#!/usr/bin/env python3
"""Isolated loopback tests; never launches/restarts a tailnet client or live node."""
import argparse
import contextlib
import io
import json
import socket
import threading
import unittest
import matrix2

class WorkloadTest(unittest.TestCase):
    def test_eight_flows_both_directions_have_actual_bytes_and_probe_window(self):
        for direction in ("d", "u"):
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            listener.listen(32)
            def serve(sock):
                with sock:
                    try:
                        mode = sock.recv(1)
                        if mode == b"d":
                            while True:
                                sock.sendall(bytes(16384))
                        else:
                            while True:
                                buf = sock.recv(16384)
                                if not buf:
                                    return
                                if mode == b"e":
                                    sock.sendall(buf)
                    except OSError:
                        pass
            workers = []
            def accept():
                for _ in range(9):
                    sock, _ = listener.accept()
                    thread = threading.Thread(target=serve, args=(sock,))
                    thread.start()
                    workers.append(thread)
            server = threading.Thread(target=accept)
            server.start()
            args = argparse.Namespace(destination=f"127.0.0.1:{listener.getsockname()[1]}",
                proxy=None, timeout=5, direction=direction, bytes=2_000_000, flows=8,
                probe_interval=0.001, node_pid=None, binary=None, flags="test", label="fixture")
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                result = matrix2.run(args)
            server.join(5)
            listener.close()
            for worker in workers:
                worker.join(5)
            row = json.loads(output.getvalue())
            self.assertEqual(result, 0)
            self.assertEqual(row["bytes"], 16_000_000)
            self.assertTrue(all(flow["bytes"] == 2_000_000 for flow in row["flows"]))
            self.assertAlmostEqual(row["seconds"], row["window_end"]-row["window_start"])
            self.assertTrue(row["probes"])
            self.assertTrue(all(row["window_start"] <= p["start"] < row["window_end"] for p in row["probes"]))

    def test_cpu_time_formats(self):
        from unittest.mock import patch
        for text, seconds in [("02:03.45", 123.45), ("01:02:03", 3723), ("2-01:02:03.5", 176523.5)]:
            with patch("subprocess.check_output", return_value=text):
                self.assertEqual(matrix2.cpu_seconds(42), seconds)

if __name__ == "__main__":
    unittest.main()
