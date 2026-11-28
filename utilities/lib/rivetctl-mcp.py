#!/usr/bin/env python3
"""rivetctl's MCP server, end to end: a scripted client session against `rivetctl mcp`, in a
throwaway home folder, with real releases from GitHub.

    python3 utilities/lib/rivetctl-mcp.py [RIVETCTL] [APP]

RIVETCTL defaults to target/debug/rivetctl, APP (installed, then removed) to snip. It starts the
server, does the handshake, lists the tools, reads the apps, asks to install APP without
confirming (nothing may change), installs it, checks for updates, removes it in two steps the
same way, runs doctor, and tries a tool that doesn't exist. Every step's answer is checked and
printed. Nothing outside the throwaway folder is touched.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

exe = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "target/debug/rivetctl")
app = sys.argv[2] if len(sys.argv) > 2 else "snip"
home = tempfile.mkdtemp(prefix="rivetctl-mcp.")
env = dict(os.environ, HOME=home, XDG_CONFIG_HOME=f"{home}/.config", XDG_DATA_HOME=f"{home}/.local/share",
           XDG_CACHE_HOME=f"{home}/.cache", PATH=f"{home}/.local/bin:/usr/local/bin:/usr/bin:/bin")
for k in ("XDG_BIN_HOME", "RIVET_WELCOME_INSTALLED", "RIVET_WELCOME_RELEASES"):
    env.pop(k, None)
srv = subprocess.Popen([exe, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
next_id = 0


def send(method, params=None, notify=False):
    global next_id
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    if not notify:
        next_id += 1
        msg["id"] = next_id
    print(">>", json.dumps(msg))
    srv.stdin.write(json.dumps(msg) + "\n")
    srv.stdin.flush()
    if notify:
        return None
    line = srv.stdout.readline()
    reply = json.loads(line)
    assert reply["id"] == next_id, reply
    shown = json.dumps(reply)
    print("<<", shown if len(shown) < 600 else shown[:600] + " …")
    return reply


def tool(name, **args):
    r = send("tools/call", {"name": name, "arguments": args})
    assert "result" in r, r
    return r["result"]


def installed():
    return os.path.exists(f"{home}/.local/bin/rivet-{app}")


try:
    r = send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "rivetctl-mcp-check", "version": "1"}})
    assert r["result"]["protocolVersion"] == "2025-06-18" and r["result"]["capabilities"]["tools"] is not None
    send("notifications/initialized", notify=True)
    assert send("ping")["result"] == {}
    names = [t["name"] for t in send("tools/list")["result"]["tools"]]
    assert names == ["list_apps", "app_status", "install_app", "update_apps", "remove_app", "doctor"], names

    apps = tool("list_apps")["structuredContent"]["apps"]
    assert any(a["id"] == app for a in apps) and not any(a["held"] for a in apps)
    s = tool("app_status", app=app)
    assert not s["isError"] and s["structuredContent"]["installed"] is None

    p = tool("install_app", app=app)
    assert not p["isError"] and p["structuredContent"]["confirmed"] is False and not installed(), p
    d = tool("install_app", app=app, confirm=True)
    assert not d["isError"] and d["structuredContent"]["done"][0]["ok"] and installed(), d
    out = subprocess.run([f"{home}/.local/bin/rivet-{app}", "--version"], capture_output=True, text=True, env=env).stdout.strip()
    print("   ", out)
    assert out.endswith(d["structuredContent"]["done"][0]["installed"])
    u = tool("update_apps", confirm=True)
    assert u["content"][0]["text"] == "Every app is up to date.", u

    p = tool("remove_app", app=app)
    assert p["structuredContent"]["confirmed"] is False and installed(), p
    d = tool("remove_app", app=app, confirm=True)
    assert not d["isError"] and not installed(), d
    gone = tool("remove_app", app=app, confirm=True)
    assert gone["isError"], gone

    held = tool("install_app", app="longshot")
    assert held["isError"] and "held back" in held["content"][0]["text"], held
    doc = tool("doctor")
    assert doc["structuredContent"]["checks"], doc
    bad = send("tools/call", {"name": "format_disk", "arguments": {}})
    assert bad["error"]["code"] == -32602
    print(f"\nrivetctl mcp passed every step (installed and removed {app}).")
finally:
    srv.stdin.close()
    srv.wait(timeout=10)
    shutil.rmtree(home, ignore_errors=True)
