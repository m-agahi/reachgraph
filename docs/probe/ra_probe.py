#!/usr/bin/env python3
"""Drive rust-analyzer over stdio LSP and ask for outgoingCalls from
task/src/service/handlers.rs::create_task.

The question: does callHierarchy cross #[tonic::async_trait] (a proc macro)
and the nested `async move` closure to report the real callees?
"""
import json, os, subprocess, sys, threading, time

ROOT = "/home/max/git/yadgarhq/task"
FILE = os.path.join(ROOT, "src/service/handlers.rs")
URI = "file://" + FILE

proc = subprocess.Popen(
    ["rust-analyzer"], cwd=ROOT,
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
)

lock = threading.Lock()
responses = {}
notifications = []
ready = threading.Event()
_id = [0]


def send(method, params, notify=False):
    msg = {"jsonrpc": "2.0", "method": method, "params": params}
    if not notify:
        _id[0] += 1
        msg["id"] = _id[0]
    body = json.dumps(msg).encode()
    proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    proc.stdin.flush()
    return msg.get("id")


def reader():
    buf = b""
    while True:
        ch = proc.stdout.read(1)
        if not ch:
            return
        buf += ch
        if buf.endswith(b"\r\n\r\n"):
            n = 0
            for line in buf.split(b"\r\n"):
                if line.lower().startswith(b"content-length:"):
                    n = int(line.split(b":")[1])
            payload = b""
            while len(payload) < n:
                payload += proc.stdout.read(n - len(payload))
            buf = b""
            try:
                m = json.loads(payload)
            except Exception:
                continue
            with lock:
                if "id" in m and ("result" in m or "error" in m):
                    responses[m["id"]] = m
                else:
                    notifications.append(m)
                    # rust-analyzer signals analysis done via $/progress end
                    if m.get("method") == "$/progress":
                        v = m.get("params", {}).get("value", {})
                        tok = str(m.get("params", {}).get("token", ""))
                        if v.get("kind") == "end" and "cachePriming" in tok:
                            ready.set()


threading.Thread(target=reader, daemon=True).start()


def wait(rid, timeout=180):
    t0 = time.time()
    while time.time() - t0 < timeout:
        with lock:
            if rid in responses:
                return responses[rid]
        time.sleep(0.05)
    return None


rid = send("initialize", {
    "processId": os.getpid(),
    "rootUri": "file://" + ROOT,
    "workspaceFolders": [{"uri": "file://" + ROOT, "name": "task"}],
    "capabilities": {
        "textDocument": {
            "callHierarchy": {"dynamicRegistration": False},
            "synchronization": {"didSave": True},
        },
        "window": {"workDoneProgress": True},
    },
})
init = wait(rid, 120)
if not init:
    print("FAIL: no initialize response")
    sys.exit(1)
caps = init["result"]["capabilities"]
print("callHierarchyProvider advertised:", caps.get("callHierarchyProvider"))
send("initialized", {}, notify=True)

src = open(FILE, encoding="utf-8").read()
send("textDocument/didOpen", {"textDocument": {
    "uri": URI, "languageId": "rust", "version": 1, "text": src}}, notify=True)

print("waiting for rust-analyzer to finish analysis ...", flush=True)
ready.wait(timeout=300)
print("  cachePriming end seen:", ready.is_set())
time.sleep(5)

# locate `async fn create_task` -> position on the identifier
lines = src.split("\n")
line_no = next(i for i, l in enumerate(lines) if "async fn create_task" in l)
col = lines[line_no].index("create_task") + 2
print(f"probe position: line {line_no} (1-indexed {line_no+1}), col {col}")
print(f"  {lines[line_no]!r}")

rid = send("textDocument/prepareCallHierarchy",
           {"textDocument": {"uri": URI}, "position": {"line": line_no, "character": col}})
prep = wait(rid)
print("\n=== prepareCallHierarchy ===")
print(json.dumps(prep.get("result") if prep else None, indent=1)[:900])

items = (prep or {}).get("result") or []
if not items:
    print("\nVERDICT: prepareCallHierarchy returned nothing — probe FAILS at step 1")
    sys.exit(2)

rid = send("callHierarchy/outgoingCalls", {"item": items[0]})
out = wait(rid)
res = (out or {}).get("result")
print("\n=== outgoingCalls ===")
if not res:
    print("EMPTY —", json.dumps(out)[:400])
else:
    for c in res:
        t = c["to"]
        uri = t.get("uri", "")
        short = uri.replace("file://" + ROOT + "/", "").replace("file://", "")
        if "/registry/" in short or "/.cargo/" in short:
            short = ".../" + "/".join(short.split("/")[-2:])
        print(f"  {t['name']:<28} {t.get('detail','') or '':<22} {short}"
              f":{t['range']['start']['line']+1}  ({len(c.get('fromRanges',[]))} site)")
print(f"\ntotal outgoing edges: {len(res or [])}")
proc.terminate()
