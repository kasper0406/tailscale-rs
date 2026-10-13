"""Minimal streamable-HTTP MCP client for the local hyperstack-mcp container.
usage: mcp.py list | mcp.py call <tool> '<json args>'"""
import json, sys, urllib.request
URL = "http://localhost:8080/mcp"
H = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
def rpc(body, sid=None):
    h = dict(H)
    if sid: h["Mcp-Session-Id"] = sid
    req = urllib.request.Request(URL, data=json.dumps(body).encode(), headers=h, method="POST")
    with urllib.request.urlopen(req, timeout=120) as r:
        sid = r.headers.get("Mcp-Session-Id") or sid
        raw = r.read().decode()
    msgs = []
    for line in raw.splitlines():
        if line.startswith("data:"): msgs.append(json.loads(line[5:]))
    if not msgs and raw.strip(): msgs.append(json.loads(raw))
    return msgs, sid
_, sid = rpc({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26",
    "capabilities": {}, "clientInfo": {"name": "claude-ts-exit", "version": "1"}}})
rpc({"jsonrpc": "2.0", "method": "notifications/initialized"}, sid)
if sys.argv[1] == "list":
    m, _ = rpc({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}, sid)
    for t in m[-1]["result"]["tools"]:
        print(t["name"], "-", (t.get("description") or "").split("\n")[0][:110])
else:
    m, _ = rpc({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": sys.argv[2],
        "arguments": json.loads(sys.argv[3]) if len(sys.argv) > 3 else {}}}, sid)
    res = m[-1].get("result", m[-1])
    for c in res.get("content", []):
        print(c.get("text", c))
    if res.get("isError"): sys.exit(1)
