#!/usr/bin/env python3
"""Mock GitHub release server for install.sh end-to-end verification.

Serves the exact four endpoints scripts/install.sh consumes:

* ``/repos/<owner>/<repo>/releases/latest``        — release JSON (tag + asset URLs)
* ``/repos/<owner>/<repo>/releases/tags/<tag>``    — same, for pinned versions
* release-asset download URLs (any ``/releases/download/...`` path or /dl/...)
* ``sha256sums.txt``                                — checksum entries

Failure modes are selected by TAG SUFFIX so a single server covers the whole
matrix without restarts:

* ``-star``      checksum entry uses the BSD ``*name`` binary form
* ``-mismatch``  checksum entry does not match the served bytes
* ``-missing``   sha256sums.txt has no entry for the asset
* ``-nosums``    release publishes no sha256sums.txt asset at all
* ``-noasset``   release JSON omits the binary URL (exercises the fallback URL)
* ``-badtag``    the release does not exist (HTTP 404)

Test-support endpoints (never counted as release queries):

* ``GET /__count``  -> ``{"release": <n>}`` — number of release-API hits
* ``GET /__reset``  -> resets the counter to 0

Every served binary download is byte-identical to what sha256sums.txt
advertises for the happy modes, so the installer's verification passes
only when its parsing is correct.
"""
import hashlib
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

BIN = b"#!/bin/sh\necho mock-awh\n"
GOOD = hashlib.sha256(BIN).hexdigest()
BAD = "0" * 64
ASSET = "awh-linux-x86_64"
STATE = {"release": 0, "last_mode": ""}
# install.sh fetches release JSON -> binary -> sha256sums.txt in order,
# so the mode chosen at release time is the one the checksum fetch must
# serve for THAT install.


def mode_for_tag(tag: str) -> str:
    for suffix in ("star", "mismatch", "missing", "nosums", "noasset", "badtag"):
        if tag.endswith("-" + suffix):
            return suffix
    return ""


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def log_message(self, *a):
        pass

    def send_bytes(self, data: bytes, ctype: str = "application/octet-stream", code: int = 200):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        base = f"http://{self.headers.get('Host', '127.0.0.1')}"
        path = self.path
        if path == "/__count":
            self.send_bytes(json.dumps({"release": STATE["release"]}).encode(), "application/json")
            return
        if path == "/__reset":
            STATE["release"] = 0
            STATE["last_mode"] = ""
            self.send_bytes(b"reset", "text/plain")
            return
        if path.endswith("/releases/latest") or "/releases/tags/" in path:
            tag = path.rstrip("/").rsplit("/", 1)[-1] or "latest"
            mode = mode_for_tag(tag)
            STATE["release"] += 1
            STATE["last_mode"] = mode
            if mode == "badtag":
                self.send_bytes(b"not found", "text/plain", 404)
                return
            if tag == "latest":
                tag = "v9.9.9"
            sums_url = f"{base}/dl/sha256sums.txt"
            assets = [{"browser_download_url": f"{base}/dl/{ASSET}"}]
            if mode != "nosums":
                assets.append({"browser_download_url": sums_url})
            if mode == "noasset":
                assets = [{"browser_download_url": sums_url}]
            doc = {"tag_name": tag, "assets": assets}
            self.send_bytes(json.dumps(doc).encode(), "application/json")
            return
        if path.endswith(f"/{ASSET}") or "/releases/download/" in path:
            self.send_bytes(BIN)
            return
        if path.endswith("/sha256sums.txt"):
            mode = STATE["last_mode"]
            if mode == "star":
                body = f"{GOOD}  *{ASSET}\n"
            elif mode == "mismatch":
                body = f"{BAD}  {ASSET}\n"
            elif mode == "missing":
                body = f"{GOOD}  some-other-asset\n"
            else:
                body = f"{GOOD}  {ASSET}\n"
            self.send_bytes(body.encode(), "text/plain")
            return
        self.send_bytes(b"not found", "text/plain", 404)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9419
    srv = HTTPServer(("127.0.0.1", port), Handler)
    print(f"listening on 127.0.0.1:{srv.server_port}", flush=True)
    srv.serve_forever()
