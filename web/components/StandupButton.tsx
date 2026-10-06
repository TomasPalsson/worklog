"use client";

import { useState } from "react";
import { draftStandup, postStandup, standupChannelSet } from "@/lib/daemonStandup";
import type { PostOutcome, StandupDraft } from "@/lib/daily_helpers_contract";

const QUESTIONS = [
  "What are you working on today?",
  "What is next/coming up?",
  "Are there any blockers we need to clear?",
];

/** Mirrors `StandupDraft::to_text` in the Rust contract. */
function draftText(d: StandupDraft): string {
  return [d.today, d.next, d.blockers]
    .map((bullets, i) => {
      const lines = bullets.length ? bullets : ["None"];
      return `${i + 1}. ${QUESTIONS[i]}\n${lines.map((b) => `• ${b}\n`).join("")}`;
    })
    .join("\n");
}

function failure(o: PostOutcome): string | null {
  switch (o.outcome) {
    case "posted":
      return null;
    case "no_channel":
      return "No Daily channel is set. Add one in Settings, or copy the text.";
    case "no_thread":
      return `No Daily thread found in ${o.channel} today. Copy the text and post it yourself.`;
    case "slack_refused":
      return `Slack refused the post: ${o.error}`;
  }
}

function Preview(p: {
  text: string;
  onText: (t: string) => void;
  canPost: boolean;
  channelSet: boolean;
  permalink: string | null;
  onPost: () => void;
  onCopy: () => void;
}) {
  return (
    <div>
      <textarea
        aria-label="Standup draft"
        rows={10}
        value={p.text}
        spellCheck={false}
        style={{ width: "100%" }}
        onChange={(e) => p.onText(e.target.value)}
      />
      {!p.channelSet && (
        <p role="status" className="settings-hint">
          No Daily channel is set, so Post is off. Set one in Settings, or copy the text.
        </p>
      )}
      <button type="button" className="action-btn" disabled={!p.canPost} onClick={p.onPost}>
        Post
      </button>{" "}
      <button type="button" className="link-btn" onClick={p.onCopy}>
        Copy
      </button>
      {p.permalink && (
        <p className="settings-hint">
          Posted. <a href={p.permalink}>Open thread</a>
        </p>
      )}
    </div>
  );
}

export function StandupButton() {
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [channelSet, setChannelSet] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [permalink, setPermalink] = useState<string | null>(null);

  const start = async () => {
    setBusy(true);
    setError(null);
    setPermalink(null);
    const [d, c] = await Promise.all([draftStandup(), standupChannelSet()]);
    setBusy(false);
    if (!d.ok) return setError(d.error);
    setChannelSet(c.ok ? c.data : true);
    setText(draftText(d.data));
  };

  const post = async () => {
    if (text === null) return;
    setBusy(true);
    setError(null);
    const r = await postStandup(text);
    setBusy(false);
    if (!r.ok) return setError(r.error);
    setError(failure(r.data));
    if (r.data.outcome === "posted") setPermalink(r.data.permalink);
  };

  const copy = async () => {
    if (text !== null) await navigator.clipboard.writeText(text);
  };

  return (
    <div className="standup" style={{ flexBasis: "100%" }}>
      <button type="button" className="action-btn" disabled={busy} onClick={() => void start()}>
        Standup
      </button>
      {error && (
        <p role="alert" className="settings-hint">
          {error}
        </p>
      )}
      {text !== null && (
        <Preview
          text={text}
          onText={setText}
          canPost={channelSet && !busy && text.trim() !== ""}
          channelSet={channelSet}
          permalink={permalink}
          onPost={() => void post()}
          onCopy={() => void copy()}
        />
      )}
    </div>
  );
}
