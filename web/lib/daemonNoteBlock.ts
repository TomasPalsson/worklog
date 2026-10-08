import { call } from "./daemon";
import type { NoteBlockBody, NoteJobStatus, RegenerateNoteResult } from "./noteBlock";
import type { RawBlock } from "./types";

export async function logNoteBlock(body: NoteBlockBody): Promise<RawBlock> {
  return call("POST", "/blocks/note", body);
}

export async function regenerateNote(id: number, force: boolean): Promise<RegenerateNoteResult> {
  return call("POST", `/blocks/${id}/note/regenerate`, { force });
}

export async function noteStatus(id: number): Promise<NoteJobStatus> {
  return call("GET", `/blocks/${id}/note/status`);
}
