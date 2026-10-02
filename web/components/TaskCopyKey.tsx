"use client";

import { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";

const SHOWN_MS = 1500;

type Result = "ok" | "failed" | null;

/** The copy-key icon: swaps to a check for a moment, or says it couldn't, then goes back. */
export function CopyKey({ taskKey, onCopied }: { taskKey: string; onCopied: (message: string) => void }) {
  const [result, setResult] = useState<Result>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function copy() {
    let next: Result = "ok";
    try {
      await navigator.clipboard.writeText(taskKey);
    } catch {
      next = "failed"; // no clipboard (insecure origin) or the browser refused
    }
    setResult(next);
    onCopied(next === "ok" ? `Copied ${taskKey}.` : `Couldn't copy ${taskKey}.`);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setResult(null), SHOWN_MS);
  }

  const word = result === "ok" ? "Copied" : result === "failed" ? "Couldn't copy" : null;
  return (
    <button
      type="button"
      className="task-icon-btn"
      aria-label={word ? `${word} ${taskKey}` : `Copy ${taskKey}`}
      data-tip={word ?? "Copy key"}
      data-result={result ?? undefined}
      onClick={copy}
    >
      {result === "ok" ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
    </button>
  );
}
