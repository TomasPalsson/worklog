"use client";

import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { draftStandup, postStandup, standupChannelSet } from "@/lib/daemonStandup";
import type { PostOutcome, StandupDraft } from "@/lib/daily_helpers_contract";

/** Mirrors `StandupDraft::to_text` in the Rust contract. */
function draftText(d: StandupDraft): string {
  return [d.today, d.next, d.blockers]
    .map((answers, i) => `${i + 1}. ${answers.length ? answers.join(" ") : "Nothing."}\n`)
    .join("");
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
      return `Slack refused the post (${o.error}). Copy the text and post it in the thread yourself.`;
  }
}

function Preview(p: {
  text: string;
  onText: (t: string) => void;
  canPost: boolean;
  channelSet: boolean;
  copied: boolean;
  permalink: string | null;
  onPost: () => void;
  onCopy: () => void;
  onDiscard: () => void;
}) {
  return (
    <div className="standup-preview">
      <textarea
        className="standup-draft"
        aria-label="Standup draft"
        rows={10}
        value={p.text}
        spellCheck={false}
        onChange={(e) => p.onText(e.target.value)}
      />
      {!p.channelSet && (
        <p role="status" className="standup-note">
          No Daily channel is set, so Post is off. Set one in Settings, or copy the text.
        </p>
      )}
      {p.channelSet && p.text.trim() === "" && (
        <p role="status" className="standup-note">
          The draft is empty, so Post is off. Write at least one line.
        </p>
      )}
      <div className="standup-actions">
        <button type="button" className="task-btn-primary" disabled={!p.canPost} onClick={p.onPost}>
          Post
        </button>
        <button type="button" className="task-btn-secondary" onClick={p.onCopy}>
          Copy
        </button>
        <button type="button" className="task-btn-secondary" onClick={p.onDiscard}>
          Discard draft
        </button>
        {p.copied && (
          <span role="status" className="standup-note">
            Copied to the clipboard
          </span>
        )}
      </div>
      {p.permalink && (
        <p role="status" className="standup-done">
          Posted to today&apos;s Daily thread. <a href={p.permalink}>Open thread</a>
        </p>
      )}
    </div>
  );
}

function useStandup() {
  const [busy, setBusy] = useState<"drafting" | "posting" | null>(null);
  const [copied, setCopied] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [channelSet, setChannelSet] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [permalink, setPermalink] = useState<string | null>(null);

  const start = async () => {
    setBusy("drafting");
    setError(null);
    setPermalink(null);
    setCopied(false);
    const [d, c] = await Promise.all([draftStandup(), standupChannelSet()]);
    setBusy(null);
    if (!d.ok) {
      setError(d.error);
      return false;
    }
    setChannelSet(c.ok ? c.data : true);
    setText(draftText(d.data));
    return true;
  };

  const post = async () => {
    if (text === null) return;
    setBusy("posting");
    setError(null);
    const r = await postStandup(text);
    setBusy(null);
    if (!r.ok) return setError(r.error);
    setError(failure(r.data));
    if (r.data.outcome === "posted") setPermalink(r.data.permalink);
  };

  const copy = async () => {
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      setError("The browser blocked the clipboard. Select the draft and copy it by hand.");
    }
  };

  return { busy, copied, text, setText, setCopied, setPermalink, channelSet, error, permalink, start, post, copy };
}

/** Opens the native modal when `open`, closes it otherwise. Falls back to the
 *  `open` attribute where `showModal` is missing (tests). */
function useModal(open: boolean) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) {
      if (typeof d.showModal === "function") d.showModal();
      else d.setAttribute("open", "");
    } else if (!open && d.open) {
      if (typeof d.close === "function") d.close();
      else d.removeAttribute("open");
    }
  }, [open]);
  return ref;
}

type Standup = ReturnType<typeof useStandup>;

/** The open draft, in a modal so the page header never grows. */
function StandupDialog(p: {
  s: Standup;
  dialogRef: React.RefObject<HTMLDialogElement | null>;
  onClose: () => void;
  onDiscard: () => void;
}) {
  const { s } = p;
  return (
    <dialog ref={p.dialogRef} className="settings-dialog standup-dialog" aria-labelledby="standup-title" onClose={p.onClose}>
      <div className="settings-header">
        <h2 id="standup-title">Standup draft</h2>
        <button type="button" className="standup-close" aria-label="Close (keeps the draft)" onClick={p.onClose}>
          <X size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="settings-body standup-body">
        {s.busy === "posting" && (
          <p role="status" className="standup-note">
            Posting to the Daily thread…
          </p>
        )}
        {s.error && (
          <p role="alert" className="standup-error">
            {s.error}
          </p>
        )}
        <Preview
          text={s.text ?? ""}
          onText={(t) => {
            s.setText(t);
            s.setCopied(false);
            s.setPermalink(null);
          }}
          canPost={s.channelSet && s.busy === null && s.permalink === null && (s.text ?? "").trim() !== ""}
          channelSet={s.channelSet}
          copied={s.copied}
          permalink={s.permalink}
          onPost={() => void s.post()}
          onCopy={() => void s.copy()}
          onDiscard={p.onDiscard}
        />
      </div>
    </dialog>
  );
}

export function StandupButton() {
  const s = useStandup();
  const [open, setOpen] = useState(false);
  const ref = useModal(open && s.text !== null);
  const hasDraft = s.text !== null;
  const discard = () => {
    s.setText(null);
    s.setCopied(false);
    s.setPermalink(null);
    setOpen(false);
  };
  return (
    <div className="standup" aria-busy={s.busy !== null}>
      <button
        type="button"
        className="action-btn"
        disabled={s.busy === "drafting"}
        onClick={() => (hasDraft ? setOpen(true) : void s.start().then((ok) => setOpen(ok)))}
      >
        {hasDraft ? "Open standup draft" : "Draft standup"}
      </button>
      {s.busy === "drafting" && (
        <p role="status" className="standup-note">
          Drafting from yesterday and today…
        </p>
      )}
      {s.error && !open && (
        <p role="alert" className="standup-error">
          {s.error}
        </p>
      )}
      {hasDraft && (
        <StandupDialog s={s} dialogRef={ref} onClose={() => setOpen(false)} onDiscard={discard} />
      )}
    </div>
  );
}
