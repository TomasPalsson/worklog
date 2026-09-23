#!/usr/bin/env python3
"""Laya classifier helper — run via `worklog laya serve`.

Binds 127.0.0.1 only. Endpoints:
  GET  /health    -> 200 (liveness probe used by `worklog laya status`)
  POST /classify  -> body {"state": <json>, "options": [<folder>, ...]}
                     response {"choice": <one of options|null>, "confidence": <0..1>}
"""
import json
import re
from http.server import BaseHTTPRequestHandler, HTTPServer

HOST = "127.0.0.1"
PORT = 9324


def tokenize(text):
    return set(re.findall(r"[a-z0-9]+", text.lower()))


def classify(state, options):
    # ponytail: keyword-overlap heuristic against the event text in
    # `state`, not the real Laya model — swap in laya's classifier once
    # its package API is confirmed (spec 003 decision 6).
    if not options:
        return None, 0.0
    state_tokens = tokenize(json.dumps(state))
    best_choice = None
    best_score = 0.0
    for option in options:
        option_tokens = tokenize(option)
        if not option_tokens:
            continue
        score = len(state_tokens & option_tokens) / len(option_tokens)
        if score > best_score:
            best_score = score
            best_choice = option
    return best_choice, min(best_score, 1.0)


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/health":
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.end_headers()

    def do_POST(self):
        if self.path != "/classify":
            self.send_response(404)
            self.end_headers()
            return
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length) or b"{}")
        choice, confidence = classify(body.get("state", {}), body.get("options", []))
        payload = json.dumps({"choice": choice, "confidence": confidence}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format, *args):  # noqa: A002 - stdlib hook signature
        pass


if __name__ == "__main__":
    HTTPServer((HOST, PORT), Handler).serve_forever()
