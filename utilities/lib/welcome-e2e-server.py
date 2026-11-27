#!/usr/bin/env python3
"""A stand-in for GitHub, for Rivet Welcome's end-to-end test (utilities/lib/welcome-e2e.sh).

It answers the two things Rivet Welcome asks GitHub for: the releases list
(/repos/OWNER/NAME/releases, in pages with Link headers and an ETag, as GitHub does) and the
downloads (/dl/NAME, with byte ranges so a dropped download can carry on). What it serves is
set by files in the folder it's given, which the test changes between steps:

  dl/NAME              the downloads
  releases.txt         one release per line, newest first: TAG pre|stable ASSET...
                       (each ASSET a file in dl/; its size and address are filled in)
  ratelimit            when present, the releases list answers 403 with no checks left
  per_page             when present, the most releases a page holds (to make several pages)
  flaky/NAME           when present, the next download of NAME stops halfway (then it's removed)
  log                  every request, written here: METHOD PATH STATUS RANGE

  server.py ROOT PORT_FILE     serve on a free port on 127.0.0.1, written to PORT_FILE
"""

import hashlib
import json
import os
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

ROOT = sys.argv[1]
LOCK = threading.Lock()


def path(*p):
    return os.path.join(ROOT, *p)


def releases(base):
    out = []
    try:
        lines = open(path("releases.txt")).read().splitlines()
    except FileNotFoundError:
        lines = []
    for line in lines:
        parts = line.split()
        if len(parts) < 2 or line.startswith("#"):
            continue
        tag, kind, assets = parts[0], parts[1], parts[2:]
        out.append({
            "tag_name": tag,
            "name": tag,
            "draft": kind == "draft",
            "prerelease": kind == "pre",
            "published_at": "2026-11-27T00:00:00Z",
            "html_url": f"{base}/releases/{tag}",
            "body": "",
            "assets": [
                {
                    "name": a,
                    "size": os.path.getsize(path("dl", a)) if os.path.exists(path("dl", a)) else 0,
                    "browser_download_url": f"{base}/dl/{a}",
                }
                for a in assets
            ],
        })
    return out


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def note(self, status):
        with LOCK, open(path("log"), "a") as f:
            f.write(f"{self.command} {self.path} {status} {self.headers.get('Range', '-')}\n")

    def send(self, status, body=b"", headers=()):
        self.send_response(status)
        for k, v in headers:
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.note(status)

    def do_GET(self):
        url = urlparse(self.path)
        base = f"http://{self.headers.get('Host')}"
        if url.path.startswith("/repos/") and url.path.endswith("/releases"):
            return self.releases(url, base)
        if url.path == "/all":
            return self.send(200, json.dumps(releases(base)).encode(), [("Content-Type", "application/json")])
        if url.path.startswith("/dl/"):
            return self.download(url.path[4:])
        self.send(404, b'{"message":"Not Found"}')

    def releases(self, url, base):
        if os.path.exists(path("ratelimit")):
            reset = int(time.time()) + 600
            return self.send(403, b'{"message":"API rate limit exceeded"}', [
                ("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", str(reset)),
            ])
        q = parse_qs(url.query)
        per = int(q.get("per_page", ["30"])[0])
        if os.path.exists(path("per_page")):
            per = min(per, int(open(path("per_page")).read()))
        page = int(q.get("page", ["1"])[0])
        everything = releases(base)
        chunk = everything[(page - 1) * per: page * per]
        body = json.dumps(chunk).encode()
        etag = '"%s"' % hashlib.sha256(body).hexdigest()[:20]
        headers = [("ETag", etag), ("x-ratelimit-remaining", "59"), ("Content-Type", "application/json")]
        if page * per < len(everything):
            headers.append(("Link", f'<{base}{url.path}?per_page={per}&page={page + 1}>; rel="next"'))
        if self.headers.get("If-None-Match") == etag:
            return self.send(304, b"", [("ETag", etag)])
        self.send(200, body, headers)

    def download(self, name):
        f = path("dl", os.path.basename(name))
        if not os.path.isfile(f):
            return self.send(404, b"Not Found")
        data = open(f, "rb").read()
        start = 0
        rng = self.headers.get("Range")
        if rng and rng.startswith("bytes="):
            start = int(rng[6:].split("-")[0] or 0)
            if start >= len(data):
                return self.send(416, b"", [("Content-Range", f"bytes */{len(data)}")])
        part = data[start:]
        flaky = path("flaky", os.path.basename(name))
        status = 206 if start else 200
        self.send_response(status)
        self.send_header("Content-Length", str(len(part)))
        if start:
            self.send_header("Content-Range", f"bytes {start}-{len(data) - 1}/{len(data)}")
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        if os.path.exists(flaky):
            os.remove(flaky)
            self.wfile.write(part[: len(part) // 2])
            self.wfile.flush()
            self.note(f"{status}-dropped")
            self.close_connection = True
            return
        self.wfile.write(part)
        self.note(status)


class Server(ThreadingHTTPServer):
    def handle_error(self, request, client_address):
        # A client that hangs up mid-answer is part of the test (and curl does it after a 304).
        pass


server = Server(("127.0.0.1", 0), Handler)
with open(sys.argv[2] + ".part", "w") as f:
    f.write(str(server.server_address[1]))
os.rename(sys.argv[2] + ".part", sys.argv[2])
server.serve_forever()
