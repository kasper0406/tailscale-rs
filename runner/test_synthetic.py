#!/usr/bin/env python3
"""Bounded isolated WG/PortGate/TCP process regression. Takes built synthetic_peer binary."""
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import unittest

BINARY = sys.argv.pop(1) if len(sys.argv) > 1 else os.environ["SYNTHETIC_PEER_BINARY"]
class SyntheticTest(unittest.TestCase):
    def test_wg_handshake_and_eight_finite_tcp_flows_each_direction(self):
        with tempfile.TemporaryDirectory() as directory:
            node = pathlib.Path(directory)/"node.key"
            peer = pathlib.Path(directory)/"peer.key"
            node_pub = subprocess.check_output([BINARY,"keygen",str(node)],text=True).strip()
            node_disco = pathlib.Path(directory)/"node-disco.key"
            peer_disco = pathlib.Path(directory)/"peer-disco.key"
            node_disco_pub = subprocess.check_output([BINARY,"keygen",str(node_disco)],text=True).strip()
            peer_disco_pub = subprocess.check_output([BINARY,"keygen",str(peer_disco)],text=True).strip()
            peer_pub = subprocess.check_output([BINARY,"keygen",str(peer)],text=True).strip()
            self.assertEqual(node.stat().st_mode & 0o777, 0o600)
            fixture = subprocess.Popen([BINARY,"--workers","2","fixture","--key-file",str(node),
                "--peer-key",peer_pub,"--disco-key-file",str(node_disco),"--peer-disco-key",peer_disco_pub],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                import selectors
                selector = selectors.DefaultSelector()
                selector.register(fixture.stdout, selectors.EVENT_READ)
                self.assertTrue(selector.select(10), "fixture startup timed out")
                selector.close()
                config=json.loads(fixture.stdout.readline())
                for direction in ("down","up"):
                    command=[BINARY,"--workers","2","peer","--wait-for-start","--key-file",str(peer),
                        "--node-key",node_pub,"--disco-key-file",str(peer_disco),"--node-disco-key",node_disco_pub,"--endpoint",config["fixture_endpoint"],
                        "--source","100.64.0.1","--destination",config["virtual_destination"],
                        "--flows","8","--bytes","1000000","--direction",direction,"--timeout","15"]
                    child=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                    try:
                        with selectors.DefaultSelector() as selector:
                            selector.register(child.stdout,selectors.EVENT_READ)
                            self.assertTrue(selector.select(20),"peer readiness timed out")
                            self.assertTrue(json.loads(child.stdout.readline())["ready"])
                            child.stdin.write("GO\n"); child.stdin.flush()
                            self.assertTrue(selector.select(20),"peer transfer timed out")
                            result=json.loads(child.stdout.readline())
                        child.stdin.write("STOP\n"); child.stdin.flush()
                        _,error=child.communicate(timeout=10)
                        self.assertEqual(child.returncode,0,error)
                    finally:
                        if child.poll() is None: child.kill()
                        child.communicate()
                    self.assertEqual(result["bytes"],8_000_000)
                    self.assertEqual(len(result["flows"]),8)
                    self.assertTrue(all(f["bytes"]==1_000_000 for f in result["flows"]))
                    self.assertGreater(result["transfer_seconds"],0)
            finally:
                fixture.send_signal(signal.SIGINT)
                try:
                    _,error=fixture.communicate(timeout=3)
                except subprocess.TimeoutExpired:
                    fixture.kill(); fixture.communicate()
                    self.fail("fixture shutdown exceeded 3 seconds")
                self.assertEqual(fixture.returncode,0,error)

if __name__ == "__main__":
    unittest.main()
