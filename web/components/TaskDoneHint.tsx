"use client";

import { useState } from "react";

import type { StatusHint } from "@/lib/types";

/** Suggests a move to Done; nothing moves until the Owner confirms. */
export function TaskDoneHint({ hint, onConfirm }: { hint: StatusHint; onConfirm: () => void }) {
  const [asking, setAsking] = useState(false);
  if (!asking) {
    return (
      <button type="button" className="task-tag task-done-hint" onClick={() => setAsking(true)}>
        Move to Done?
      </button>
    );
  }
  const { repo, number } = hint.reason;
  return (
    <div className="task-done-hint" role="group" aria-label={`Move ${hint.key} to Done`}>
      <span>{`${repo}#${number} merged.`}</span>
      <button type="button" onClick={onConfirm}>
        Move to Done
      </button>
      <button type="button" onClick={() => setAsking(false)}>
        Not now
      </button>
    </div>
  );
}
