#!/usr/bin/env python3
"""Laya classifier helper — run via `worklog laya serve`.

Binds 127.0.0.1 only. Endpoints:
  GET  /health    -> 200 (liveness probe used by `worklog laya status`)
  POST /classify  -> body {"state": <json>, "options": [<folder>, ...]}
                     response {"choice": <one of options>, "confidence": <0..1>}
                     empty options -> 400
"""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

from laya import Router

HOST = "127.0.0.1"
PORT = 9324

# Loaded once at process startup, not per request — the model load is
# the expensive part (spec 003 A5).
router = Router(preload=True)


def classify(state, options):
    questions = {
        "project": {
            "type": "choice",
            "instructions": "Which project does this work activity belong to?",
            "criteria": {option: option for option in options},
        }
    }
    result = router.predict(state, questions)
    answer = result["answers"]["project"]
    return answer["choice"], answer["confidence"]


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
        options = body.get("options", [])
        if not options:
            self.send_response(400)
            self.end_headers()
            return
        choice, confidence = classify(body.get("state", {}), options)
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
