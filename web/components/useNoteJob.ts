"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { noteStatusAction } from "@/app/actions-note-block";
import { NOTE_POLL_MAX_MS, NOTE_POLL_MS } from "@/lib/noteBlock";
import { toast } from "@/lib/toast";

/** Polls each tracked note job until it settles or NOTE_POLL_MAX_MS passes. */
export function useNoteJob(day: string): { track: (blockId: number) => void; running: Set<number> } {
  const router = useRouter();
  const [running, setRunning] = useState<Set<number>>(new Set());
  const cancels = useRef(new Set<() => void>());

  useEffect(() => {
    const live = cancels.current;
    return () => {
      for (const cancel of live) cancel();
      live.clear();
      setRunning(new Set());
    };
  }, [day]);

  const track = useCallback(
    (blockId: number) => {
      setRunning((r) => new Set(r).add(blockId));
      let polls = 0;
      let settled = false;
      const settle = () => {
        settled = true;
        clearInterval(timer);
        cancels.current.delete(settle);
        setRunning((r) => {
          const n = new Set(r);
          n.delete(blockId);
          return n;
        });
      };
      const timer = setInterval(async () => {
        polls += 1;
        const res = await noteStatusAction(blockId);
        if (settled) return;
        if (res.ok && res.data.state === "done") {
          settle();
          router.refresh();
        } else if (res.ok && res.data.state === "failed") {
          settle();
          // The Owner's hand edit stays and the AI write is dropped: not an error.
          if (res.data.reason !== "hand-edited")
            toast.error(`The AI could not write the description${res.data.reason ? ` — ${res.data.reason}` : ""}`);
        } else if (polls * NOTE_POLL_MS >= NOTE_POLL_MAX_MS) {
          settle();
          toast.notice("still writing — check back");
        }
      }, NOTE_POLL_MS);
      cancels.current.add(settle);
    },
    [router],
  );

  return { track, running };
}
