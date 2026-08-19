#!/usr/bin/env python3
"""Small GitHub REST fixture used by the hosted-container smoke test."""

import argparse
import base64
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse


FILES = {
    "base-spec": b"# Sample specification\n\n## Requirement\nThe dashboard MUST show canonical specs.\n",
    "base-proposal": b"# Canonical change\n\n## Why\nExercise the base snapshot.\n",
    "base-proposal-v2": b"# Canonical change\n\n## Why\nExercise a webhook refresh.\n",
    "base-tasks": b"# Tasks\n\n- [ ] Complete canonical work\n",
    "pr-proposal": b"# Pull request change\n\n## Why\nExercise PR provenance.\n",
    "pr-tasks": b"# Tasks\n\n- [x] Add fixture\n",
}


def blob(name):
    content = FILES[name]
    return {
        "encoding": "base64",
        "size": len(content),
        "content": base64.b64encode(content).decode("ascii"),
    }


def tree(entries):
    return {
        "truncated": False,
        "tree": [
            {
                "path": path,
                "mode": "100644",
                "type": "blob",
                "sha": sha,
                "size": len(FILES[sha]),
            }
            for path, sha in entries
        ],
    }


class Handler(BaseHTTPRequestHandler):
    state_lock = threading.Lock()
    failed = False
    version = 1

    def log_message(self, _format, *_args):
        return

    def send_json(self, value, status=200):
        payload = json.dumps(value).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self):
        if self.path == "/__fixture/fail":
            with self.state_lock:
                Handler.failed = True
            self.send_json({"failed": True})
            return
        if self.path == "/__fixture/recover":
            with self.state_lock:
                Handler.failed = False
            self.send_json({"failed": False})
            return
        if self.path == "/__fixture/version/2":
            with self.state_lock:
                Handler.version = 2
            self.send_json({"version": 2})
            return
        if self.failed:
            self.send_json({"message": "fixture unavailable"}, 503)
            return
        if self.path == "/app/installations/2/access_tokens":
            self.send_json({"token": "fixture-token", "expires_at": "2099-01-01T00:00:00Z"})
            return
        self.send_json({"message": "not found"}, 404)

    def do_GET(self):
        if self.failed:
            self.send_json({"message": "fixture unavailable"}, 503)
            return
        parsed = urlparse(self.path)
        path = parsed.path
        query = parsed.query

        if "/git/ref/heads/" in path:
            sha = "base-sha-v2" if self.version == 2 else "base-sha"
            self.send_json({"object": {"sha": sha}})
        elif path.endswith("/pulls") and "state=open" in query:
            self.send_json(
                [
                    {
                        "number": 12,
                        "html_url": "https://github.com/ToruAI/openspec-ui/pull/12",
                        "head": {"ref": "describe-pr-change", "sha": "pr-sha"},
                        "base": {"ref": "demo/main", "sha": "base-sha"},
                        "merged_at": None,
                    }
                ]
            )
        elif path.endswith("/pulls") and "state=closed" in query:
            self.send_json(
                [
                    {
                        "number": 9,
                        "html_url": "https://github.com/ToruAI/openspec-ui/pull/9",
                        "head": {"ref": "canonical-change", "sha": "merged-sha"},
                        "base": {"ref": "demo/main", "sha": "base-sha"},
                        "merged_at": "2026-07-01T00:00:00Z",
                    }
                ]
            )
        elif path.endswith("/pulls/9/files"):
            self.send_json(
                [{"filename": "openspec/changes/canonical-change/proposal.md"}]
            )
        elif path.endswith("/git/trees/base-sha"):
            self.send_json(
                tree(
                    [
                        ("openspec/specs/sample/spec.md", "base-spec"),
                        ("openspec/changes/canonical-change/proposal.md", "base-proposal"),
                        ("openspec/changes/canonical-change/tasks.md", "base-tasks"),
                    ]
                )
            )
        elif path.endswith("/git/trees/pr-sha"):
            self.send_json(
                tree(
                    [
                        ("openspec/changes/pull-request-change/proposal.md", "pr-proposal"),
                        ("openspec/changes/pull-request-change/tasks.md", "pr-tasks"),
                    ]
                )
            )
        elif path.endswith("/git/trees/base-sha-v2"):
            self.send_json(
                tree(
                    [
                        ("openspec/specs/sample/spec.md", "base-spec"),
                        ("openspec/changes/canonical-change/proposal.md", "base-proposal-v2"),
                        ("openspec/changes/canonical-change/tasks.md", "base-tasks"),
                    ]
                )
            )
        elif "/git/blobs/" in path:
            self.send_json(blob(path.rsplit("/", 1)[-1]))
        else:
            self.send_json({"message": "not found"}, 404)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    args = parser.parse_args()
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
