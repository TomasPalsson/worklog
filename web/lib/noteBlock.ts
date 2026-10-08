// Shared types for note blocks (spec 019) — mirrors
// rust/crates/worklog-core/src/note_block_contract.rs. Owned by the spec;
// task code imports from here and never redeclares.

/** Who wrote a block's current description; null on non-note blocks. */
export type DescriptionOrigin = "note" | "ai" | "hand";

/** The two block fields a note block adds; read as `Block & NoteFields`. */
export interface NoteFields {
  rough_note?: string | null;
  description_origin?: DescriptionOrigin | null;
}

/** `POST /blocks/note` body. `start` is local HH:MM. */
export interface NoteBlockBody {
  jira_issue: string;
  day: string;
  start: string;
  minutes: number;
  note: string;
}

export interface RegenerateNoteResult {
  started: boolean;
  /** Why the job didn't start, e.g. "already running". */
  reason?: string;
}

export type NoteJobState = "idle" | "running" | "done" | "failed";

export interface NoteJobStatus {
  state: NoteJobState;
  /** Present when `state` is "failed", e.g. "hand-edited". */
  reason?: string;
}

export const NOTE_MAX_CHARS = 500;
export const NOTE_MAX_MINUTES = 720;
export const NOTE_POLL_MS = 1000;
export const NOTE_POLL_MAX_MS = 30_000;
export const TICKET_KEY_RE = /^[A-Z][A-Z0-9]+-\d+$/;
