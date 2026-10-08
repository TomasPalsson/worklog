"use client";

import { regenerateNoteAction } from "@/app/actions-note-block";
import type { NoteFields } from "@/lib/noteBlock";
import { toast } from "@/lib/toast";
import type { Block } from "@/lib/types";
import { useNoteJob } from "./useNoteJob";

/** Regenerate click plus a `writing` flag for a note block; a hand-edited description needs a confirm first. */
export function useNoteRegenerate(block: Block & NoteFields, day: string): { regenerate: () => void; writing: boolean } {
  const { track, running } = useNoteJob(day);

  const run = async (force: boolean) => {
    const r = await regenerateNoteAction(block.id, day, force);
    if (!r.ok) return toast.error(`Regenerate failed — ${r.error}`);
    if (!r.data.started) return toast.error(`Not regenerated — ${r.data.reason ?? "job did not start"}`);
    track(block.id);
  };

  const regenerate = () => {
    if (block.description_origin === "hand") {
      toast.notice("Replace your edit?", { label: "Replace", onClick: () => void run(true) });
    } else {
      void run(false);
    }
  };

  return { regenerate, writing: running.has(block.id) };
}
