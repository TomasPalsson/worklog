"use client";

import { useEffect, useState, type FormEvent } from "react";
import type { ActionResult } from "@/app/actions";
import { MAX_INSTRUCTION_CHARS } from "@/lib/redescribe";
import { toast } from "@/lib/toast";

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

  return (
    <form className="block-context-menu-instruction" onSubmit={submit}>
      <input
        autoFocus
        type="text"
        aria-label="Instruction"
        maxLength={MAX_INSTRUCTION_CHARS}
        value={instruction}
        readOnly={writing}
        onChange={(e) => setInstruction(e.target.value)}
      />
      <button type="submit" disabled={writing || !trimmed || [...trimmed].length > MAX_INSTRUCTION_CHARS}>
        {writing ? "Writing…" : "Regenerate"}
      </button>
    </form>
  );
}
