#!/usr/bin/env python3
"""Verdict classifier helper — run via `worklog verdict serve`.

Binds 127.0.0.1 only. Endpoints:
  GET  /health    -> 200 (liveness probe used by `worklog verdict status`)
  POST /classify  -> body {"state": <json>, "options": [<folder>, ...],
                           "examples": {<folder>: [<text>, ...]}}  (examples optional)
                     response {"ranking": [{"id", "probability"}, ...<=3],
                               "abstain": <0..1>, "agreed": <bool>}
                     agreed: the reversed-order answer has the same top choice.
                     empty options -> 400
  POST /match     -> body {"query": <text>, "states": [<json>, ...]}
                     response {"matches": [<bool>, ...]} (one per state; used by `worklog eval`)
                     empty query -> 400

The callers send at most 6 options, so no grouping is needed. `--self-test`
runs on fakes with the system python3 — it never imports rlcd.
"""
import functools
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

HOST = "127.0.0.1"
PORT = 9324
RANKING_MAX = 3
SHORTLIST_MAX = 6
MODEL_TOKENS = 512
EXAMPLES_PER_OPTION = 5
EXAMPLE_CHARS = 60
EXAMPLE_CHARS_TOTAL = 300
INSUFFICIENT_EVIDENCE_ID = "__insufficient_evidence__"
# The wording the 4-right / 0-wrong measurement used; the default ratios are tuned to it.
QUESTION = "Which project folder does this work activity belong to? Links, repo names and paths are the strongest evidence."

ENGINE = None


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


def classify(state, options, examples=None):
    if len(options) == 1:
        # Verdict favours a lone named option whatever the text says (measured: a PR link
        # 0.63 vs "pool" 0.56), so one candidate is never guessed — a rule covers it.
        return {"ranking": [{"id": options[0], "probability": 0.0}], "abstain": 1.0, "agreed": False}
    context = json.dumps(state)
    capped = _cap_examples(options, examples or {})
    probabilities = _ask(context, options, capped)
    flipped = _ask(context, options[::-1], capped)
    ranked = sorted(
        ((option, probability) for option, probability in probabilities.items() if option != INSUFFICIENT_EVIDENCE_ID),
        key=lambda pair: -pair[1],
    )
    return {
        "ranking": [{"id": option, "probability": probability} for option, probability in ranked[:RANKING_MAX]],
        "abstain": probabilities.get(INSUFFICIENT_EVIDENCE_ID, 0.0),
        "agreed": _top(probabilities) == _top(flipped),
    }


def _top(probabilities):
    return max((item for item in probabilities.items() if item[0] != INSUFFICIENT_EVIDENCE_ID), key=lambda item: item[1])[0]


def _ask(context, options, capped):
    return _probabilities(context, tuple(options), tuple(capped.get(option, ()) for option in options))


def _cap_examples(options, examples):
    # Capped once on the original order so both passes show the model identical text.
    capped, total = {}, 0
    for option in options:
        kept = []
        for text in examples.get(option, [])[:EXAMPLES_PER_OPTION]:
            text = text[:EXAMPLE_CHARS]
            if total + len(text) > EXAMPLE_CHARS_TOTAL:
                break
            kept.append(text)
            total += len(text)
        capped[option] = tuple(kept)
    return capped


def _describe(option, texts):
    description = f"work in the project folder ~/Desktop/Work/{option}"
    if texts:
        description += ". Past work here: " + "; ".join(texts)
    return description


# The model is deterministic (same input, same probabilities), and the 15-min tick asks
# again about every event it left unsorted, so an answer is computed once per server.
@functools.lru_cache(maxsize=4096)
def _probabilities(context, options, examples):
    from rlcd import Choice, Option

    query = Choice(
        id="0",
        question=QUESTION,
        options=tuple(Option(id=option, description=_describe(option, texts)) for option, texts in zip(options, examples)),
    )
    return ENGINE.evaluate(context, [query]).results[0].probabilities


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
        if self.path not in ("/classify", "/match"):
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
        options = body.get("options", [])
        if not options:
            self.send_response(400)
            self.end_headers()
            return
        self._send_json(classify(body.get("state", {}), options, body.get("examples")))

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
    assert not hasattr(sys.modules[__name__], "split_groups")
    assert not hasattr(sys.modules[__name__], "merge_groups")

    calls = []
    answers = {}

    def fake(context, options, examples):
        calls.append((options, examples))
        return answers[options[0]]

    global _probabilities
    real, _probabilities = _probabilities, fake
    try:
        # Lone option: a rule, no model call; agreed stays false.
        assert classify({}, ["only-one"]) == {
            "ranking": [{"id": "only-one", "probability": 0.0}],
            "abstain": 1.0,
            "agreed": False,
        }
        assert calls == []

        ev = INSUFFICIENT_EVIDENCE_ID
        original = {"a": 0.1, "b": 0.5, "c": 0.2, "d": 0.15, ev: 0.05}
        reversed_ = {"a": 0.1, "b": 0.4, "c": 0.2, "d": 0.2, ev: 0.1}
        answers.update({"a": original, "d": reversed_})
        out = classify({}, ["a", "b", "c", "d"])
        # top 3 only, best first (catches no truncation / unsorted)
        assert [r["id"] for r in out["ranking"]] == ["b", "c", "d"]
        assert out["ranking"][0]["probability"] == 0.5
        # abstain comes from the original order (catches reading the reversed answer)
        assert out["abstain"] == 0.05
        assert out["agreed"] is True
        # second pass sees the options reversed (catches asking the same order twice)
        assert [c[0] for c in calls] == [("a", "b", "c", "d"), ("d", "c", "b", "a")]

        # exactly 3 and 2 options: no padding, no off-by-one on the cut
        answers["x"] = {"x": 0.6, "y": 0.3, "z": 0.1, ev: 0.0}
        answers["z"] = answers["x"]
        assert len(classify({}, ["x", "y", "z"])["ranking"]) == 3
        answers["q"] = {"p": 0.7, "q": 0.2, ev: 0.1}
        answers["p"] = answers["q"]
        assert [r["id"] for r in classify({}, ["p", "q"])["ranking"]] == ["p", "q"]

        # reversed order picks a different top -> not agreed (catches agreed always true)
        answers["m"] = {"m": 0.6, "n": 0.3, ev: 0.1}
        answers["n"] = {"m": 0.3, "n": 0.6, ev: 0.1}
        assert classify({}, ["m", "n"])["agreed"] is False

        # examples: 60-char cut, 5 per option, 300 total, same text both passes
        calls.clear()
        answers["o0"] = answers["o10"] = {"o0": 0.5, "o10": 0.4, ev: 0.1}
        names = [f"o{i}" for i in range(11)]
        for name in names:
            answers[name] = answers["o0"]
        classify({}, names, {"o0": ["e" * 60, "f" * 61, "g", "h", "i", "j"], "o1": ["k" * 60] * 5, "o2": ["l" * 60] * 5, "o3": ["m" * 60] * 5})
        first = dict(zip(calls[0][0], calls[0][1]))
        second = dict(zip(calls[1][0], calls[1][1]))
        assert first == second
        assert first["o0"] == ("e" * 60, "f" * 60, "g", "h", "i")
        assert sum(len(t) for texts in first.values() for t in texts) <= EXAMPLE_CHARS_TOTAL
        assert sum(len(t) for texts in first.values() for t in texts) > EXAMPLE_CHARS_TOTAL - EXAMPLE_CHARS

        # no examples key -> none shown
        calls.clear()
        classify({}, ["m", "n"])
        assert calls[0][1] == ((), ())
    finally:
        _probabilities = real

    # option text carries the examples, and still names the folder
    description = _describe("vitinn", ("deploy notes",))
    assert "~/Desktop/Work/vitinn" in description and "deploy notes" in description
    assert _describe("vitinn", ()) == "work in the project folder ~/Desktop/Work/vitinn"

    _event_text_room()
    _http_round_trip()
    print("verdict_server self-test OK")


def _event_text_room():
    # Worst case: SHORTLIST_MAX options with 32-char folder ids, the question, and the full
    # example budget as Slack/Jira-shaped titles. rlcd packs all of it plus the event into one
    # MODEL_TOKENS sequence; the event needs 200. Real tokenizer when available, else 3 chars/token.
    titles = ["PROJ-1234 Re: prod deploy failing on the staging pipeline"[:EXAMPLE_CHARS].ljust(EXAMPLE_CHARS, ".")] * (EXAMPLE_CHARS_TOTAL // EXAMPLE_CHARS)
    options = ["customer-portal-frontend-app-0" + str(i) + "x" for i in range(SHORTLIST_MAX)]
    assert all(len(option) == 32 for option in options)
    block = QUESTION + "".join(_describe(option, tuple(titles) if i == 0 else ()) for i, option in enumerate(options))
    tokenizer_file = os.path.join(os.environ.get("WORKLOG_VERDICT_MODEL_DIR") or os.path.expanduser("~/.local/share/worklog/verdict-model"), "tokenizer.json")
    try:
        from tokenizers import Tokenizer

        used = len(Tokenizer.from_file(tokenizer_file).encode(block).ids) if os.path.exists(tokenizer_file) else -(-len(block) // 3)
    except ImportError:
        used = -(-len(block) // 3)
    assert MODEL_TOKENS - used >= 200, "option block crowds out the event text"


def _http_round_trip():
    import threading
    import urllib.error
    import urllib.request

    global _probabilities
    real, _probabilities = _probabilities, lambda context, options, examples: {"a": 0.9, "b": 0.05, INSUFFICIENT_EVIDENCE_ID: 0.05}
    server = HTTPServer((HOST, 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()

    def post(body):
        request = urllib.request.Request(f"http://{HOST}:{server.server_port}/classify", json.dumps(body).encode())
        return urllib.request.urlopen(request)

    try:
        reply = json.load(post({"state": {}, "options": ["a", "b"]}))
        assert reply["ranking"][0] == {"id": "a", "probability": 0.9}
        assert reply["abstain"] == 0.05 and reply["agreed"] is True
        assert set(reply) == {"ranking", "abstain", "agreed"}
        try:
            post({"state": {}, "options": []})
            raise AssertionError("empty options must be rejected")
        except urllib.error.HTTPError as error:
            assert error.code == 400
    finally:
        server.shutdown()
        _probabilities = real


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
