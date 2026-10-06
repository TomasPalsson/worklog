#!/usr/bin/env python3
"""Verdict classifier helper — run via `worklog verdict serve`.

Binds 127.0.0.1 only. Endpoints:
  GET  /health    -> 200 (liveness probe used by `worklog verdict status`)
  POST /classify  -> body {"state": <json>, "options": [<folder>, ...]}
                     response {"choice": <one of options>, "probability": <0..1>,
                                "runner_up": <0..1>, "abstain": <0..1>}
                     empty options -> 400
  POST /match     -> body {"query": <text>, "states": [<json>, ...]}
                     response {"matches": [<bool>, ...]} (one per state; used by `worklog eval`)
                     empty query -> 400
  POST /pick      -> body {"text": <str>, "options": [{"id": <ticket key>, "description": <summary>}, ...]}
                     response same shape as /classify (choice is an option id)
                     blank text or fewer than 2 options -> 400

Verdict caps a single Choice query at 24 real options (plus its
abstention option). `split_groups`/`merge_groups` below split larger
option lists into groups and merge the per-group results back into one
winner/runner-up/abstain triple, compared by raw probability across
groups (no renormalisation). `--self-test` exercises those two pure
functions on fakes, so it runs with the system python3 — it never
imports rlcd.
"""
import functools
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

HOST = "127.0.0.1"
PORT = 9324
MAX_GROUP_SIZE = 24
INSUFFICIENT_EVIDENCE_ID = "__insufficient_evidence__"
# The wording the 4-right / 0-wrong measurement used; the default ratios are tuned to it.
QUESTION = "Which project folder does this work activity belong to? Links, repo names and paths are the strongest evidence."

PICK_QUESTION = "Which Jira ticket is this coding session working on? Ticket keys, branch names and the request's own words are the strongest evidence."

ENGINE = None


def split_groups(options):
    groups = [options[i : i + MAX_GROUP_SIZE] for i in range(0, len(options), MAX_GROUP_SIZE)]
    # A Choice needs at least 2 options: borrow one from the previous full group.
    if len(groups) > 1 and len(groups[-1]) == 1:
        groups[-1] = groups[-2][-1:] + groups[-1]
        groups[-2] = groups[-2][:-1]
    return groups


def merge_groups(group_probabilities):
    winner_id, winner_prob, runner_up, abstain = None, 0.0, 0.0, 0.0
    for probabilities in group_probabilities:
        abstain = max(abstain, probabilities.get(INSUFFICIENT_EVIDENCE_ID, 0.0))
        for option_id, probability in probabilities.items():
            if option_id == INSUFFICIENT_EVIDENCE_ID:
                continue
            if probability > winner_prob:
                winner_id, winner_prob, runner_up = option_id, probability, winner_prob
            elif probability > runner_up:
                runner_up = probability
    return winner_id, winner_prob, runner_up, abstain


def _load_engine():
    from huggingface_hub import snapshot_download
    from rlcd import DecisionEngine

    model_dir = os.environ["WORKLOG_VERDICT_MODEL_DIR"]
    snapshot_download(
        os.environ["WORKLOG_VERDICT_MODEL_REPO"],
        revision=os.environ["WORKLOG_VERDICT_MODEL_REVISION"],
        local_dir=model_dir,
        allow_patterns=[
            "config.json",
            "model.safetensors",
            "model.onnx",
            "tokenizer.json",
            "tokenizer_config.json",
            "calibrator.json",
        ],
    )
    return DecisionEngine(model_name_or_path=model_dir, device="cpu")


def classify(state, options):
    if len(options) == 1:
        # Verdict favours a lone named option whatever the text says (measured: a PR link
        # 0.63 vs "pool" 0.56), so one candidate is never guessed — a rule covers it.
        return options[0], 0.0, 0.0, 1.0
    return _evaluate(json.dumps(state), tuple(options))


# The model is deterministic (same input, same probabilities), and the 15-min tick asks
# again about every event it left unsorted, so an answer is computed once per server.
@functools.lru_cache(maxsize=4096)
def _evaluate(context, options):
    from rlcd import Choice, Option

    queries = [
        Choice(
            id=str(index),
            question=QUESTION,
            options=tuple(Option(id=option, description=f"work in the project folder ~/Desktop/Work/{option}") for option in group),
        )
        for index, group in enumerate(split_groups(options))
    ]
    batch = ENGINE.evaluate(context, queries)
    return merge_groups(result.probabilities for result in batch.results)


def pick(text, options):
    return _pick(text, tuple((o["id"], o["description"]) for o in options))


@functools.lru_cache(maxsize=4096)
def _pick(text, options):
    from rlcd import Choice, Option

    queries = [
        Choice(
            id=str(index),
            question=PICK_QUESTION,
            options=tuple(Option(id=key, description=f"{key}: {summary}") for key, summary in group),
        )
        for index, group in enumerate(split_groups(options))
    ]
    batch = ENGINE.evaluate(text, queries)
    return merge_groups(result.probabilities for result in batch.results)


def match(query, states):
    return [_match_one(json.dumps(state), query) for state in states]


# A two-way Choice, not a Noul: measured on real blocks, Noul said "true" to
# everything, while "<query>" vs "something other than <query>" separated them.
@functools.lru_cache(maxsize=4096)
def _match_one(context, query):
    from rlcd import Choice, Option

    choice = Choice(
        id="match",
        question="What is this work activity about?",
        options=(
            Option(id="match", description=query),
            Option(id="other", description=f"something other than {query}"),
        ),
    )
    return ENGINE.evaluate(context, [choice]).results[0].selected_id == "match"


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/health":
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.end_headers()

    def do_POST(self):
        if self.path not in ("/classify", "/match", "/pick"):
            self.send_response(404)
            self.end_headers()
            return
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length) or b"{}")
        if self.path == "/match":
            query = body.get("query", "").strip()
            if not query:
                self.send_response(400)
                self.end_headers()
                return
            self._send_json({"matches": match(query, body.get("states", []))})
            return
        if self.path == "/pick":
            text = str(body.get("text", "")).strip()
            options = body.get("options", [])
            if not text or len(options) < 2:
                self.send_response(400)
                self.end_headers()
                return
            choice, probability, runner_up, abstain = pick(text, options)
            self._send_json({"choice": choice, "probability": probability, "runner_up": runner_up, "abstain": abstain})
            return
        options = body.get("options", [])
        if not options:
            self.send_response(400)
            self.end_headers()
            return
        choice, probability, runner_up, abstain = classify(body.get("state", {}), options)
        self._send_json(
            {
                "choice": choice,
                "probability": probability,
                "runner_up": runner_up,
                "abstain": abstain,
            }
        )

    def _send_json(self, body):
        payload = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format, *args):  # noqa: A002 - stdlib hook signature
        pass


def self_test():
    options = [f"project-{i}" for i in range(44)]
    groups = split_groups(options)
    assert [len(group) for group in groups] == [24, 20]
    assert sorted(option for group in groups for option in group) == sorted(options)
    assert [len(group) for group in split_groups(options[:25])] == [23, 2]
    assert [len(group) for group in split_groups(options[:2])] == [2]
    grouped = split_groups([f"project-{i}" for i in range(49)])
    assert [len(group) for group in grouped] == [24, 23, 2]
    assert [option for group in grouped for option in group] == [f"project-{i}" for i in range(49)]

    winner, probability, runner_up, abstain = merge_groups(
        [
            {"project-0": 0.9, "project-1": 0.05, INSUFFICIENT_EVIDENCE_ID: 0.05},
            {"project-30": 0.6, "project-31": 0.1, INSUFFICIENT_EVIDENCE_ID: 0.3},
        ]
    )
    assert winner == "project-0"
    assert probability == 0.9
    assert runner_up == 0.6
    assert abstain == 0.3

    assert classify({}, ["only-one"]) == ("only-one", 0.0, 0.0, 1.0)

    print("verdict_server self-test OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return
    global ENGINE
    # Loaded once at process startup, not per request — the model load is
    # the expensive part (spec 003 A5).
    ENGINE = _load_engine()
    HTTPServer((HOST, PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
