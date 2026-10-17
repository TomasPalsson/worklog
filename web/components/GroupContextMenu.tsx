"use client";

import { useEffect, useRef, useState } from "react";
import { redescribeTempoLineText } from "@/app/actions-tempo-lines";
import type { TempoLineKey } from "@/lib/tempo_line_contract";
import { InstructionBox } from "./InstructionBox";
import { menuKeys } from "./menuKeys";
import { useMenuClamp } from "./useMenuClamp";
import { useMenuDismiss } from "./useMenuDismiss";

interface Props {
  /** Absent for the unassigned group (and a ticket with no line yet). */
  lineKey: TempoLineKey | undefined;
  x: number;
  y: number;
  close: () => void;
}

/** The right-click menu on a super block header: rewrite the group's line text from an instruction. */
export function GroupContextMenu({ lineKey, x, y, close }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [boxOpen, setBoxOpen] = useState(false);
  const [writing, setWriting] = useState(false);

  // While the AI writes, an outside click must not lose the request; the box handles Esc itself.
  useMenuDismiss(!writing, menuRef, close);
  const pos = useMenuClamp(menuRef, x, y, boxOpen);
  useEffect(() => {
    menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
  }, []);

  return (
    <div ref={menuRef} role="menu" className="block-context-menu" style={pos} onKeyDown={menuKeys}>
      {boxOpen && lineKey ? (
        <InstructionBox
          onSubmit={(instruction) => redescribeTempoLineText(lineKey, instruction)}
          close={close}
          onWriting={setWriting}
          successMessage="Text rewritten"
          failurePrefix="Rewrite failed"
        />
      ) : (
        <button
          type="button"
          role="menuitem"
          disabled={!lineKey}
          title={lineKey ? undefined : "Pick a ticket first"}
          onClick={() => setBoxOpen(true)}
        >
          Regenerate with prompt…
        </button>
      )}
    </div>
  );
}
