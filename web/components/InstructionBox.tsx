"use client";

import { useEffect, useId, useState, type FormEvent } from "react";
import { Loader2 } from "lucide-react";
import type { ActionResult } from "@/app/actions";
import { MAX_INSTRUCTION_CHARS } from "@/lib/redescribe";
import { toast } from "@/lib/toast";
import { RewriteSpark } from "./icons/RewriteSpark";

/** The "N left" counter shows once this few characters remain. */
const COUNTER_FROM = 100;

/** Replaces a menu's item list: the one-time instruction for "Regenerate with prompt…". */
export function InstructionBox({ onSubmit, close, onWriting, successMessage, failurePrefix }: {
  onSubmit: (instruction: string) => Promise<ActionResult<unknown>>;
  close: () => void;
  onWriting: (writing: boolean) => void;
  successMessage: string;
  failurePrefix: string;
}) {
  const [instruction, setInstruction] = useState("");
  const [writing, setWriting] = useState(false);
  const id = useId();
  const trimmed = instruction.trim();

  // The menu's own dismiss is off while writing, so Esc is handled here.
  useEffect(() => {
    if (!writing) return;
    const onKey = (e: globalThis.KeyboardEvent) => e.key === "Escape" && close();
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [writing, close]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setWriting(true);
    onWriting(true);
    let r: ActionResult<unknown>;
    try {
      r = await onSubmit(trimmed);
    } catch (err) {
      r = { ok: false, error: err instanceof Error ? err.message : String(err) };
    } finally {
      setWriting(false);
      onWriting(false);
    }
    if (!r.ok) return toast.error(`${failurePrefix} — ${r.error}`);
    toast.ok(successMessage);
    close();
  };

  const left = MAX_INSTRUCTION_CHARS - [...instruction].length;
  const canSubmit = !writing && !!trimmed && [...trimmed].length <= MAX_INSTRUCTION_CHARS;

  return (
    <form className="instruction-box" onSubmit={submit} aria-busy={writing}>
      <div className="instruction-head">
        <label htmlFor={`${id}-input`}>Instruction</label>
        {left <= COUNTER_FROM && <span className="instruction-count">{left} left</span>}
      </div>
      <input
        id={`${id}-input`}
        autoFocus
        type="text"
        placeholder="e.g. say it was the login bug fix"
        aria-describedby={`${id}-hint`}
        maxLength={MAX_INSTRUCTION_CHARS}
        value={instruction}
        readOnly={writing}
        onChange={(e) => setInstruction(e.target.value)}
      />
      <p id={`${id}-hint`} className="instruction-hint" aria-live="polite">
        {writing
          ? "Writing — hiding this won't stop it."
          : "Only the text changes. Hours stay the same."}
      </p>
      <div className="instruction-foot">
        <span className="instruction-keys" aria-hidden="true">
          {canSubmit && <><kbd>↵</kbd> regenerate </>}<kbd>esc</kbd> {writing ? "hide" : "close"}
        </span>
        <button type="submit" disabled={!canSubmit}>
          {writing ? <Loader2 className="spin" width={14} height={14} aria-hidden="true" /> : <RewriteSpark width={14} height={14} />}
          {writing ? "Writing…" : "Regenerate"}
        </button>
      </div>
    </form>
  );
}
