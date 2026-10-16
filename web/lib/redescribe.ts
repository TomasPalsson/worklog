// Shared types for "Regenerate with prompt" (spec 026) — mirrors
// rust/crates/worklog-core/src/redescribe_contract.rs. Owned by the spec;
// task code imports from here and never redeclares.

/** Longest instruction the Owner may send, in chars, after trimming. */
export const MAX_INSTRUCTION_CHARS = 500;

/** `POST /blocks/:id/redescribe` body. */
export interface RedescribeBody {
  instruction: string;
}

/** `POST /blocks/:id/redescribe` 200 body. Only the description changed. */
export interface RedescribeResult {
  block_id: number;
  description: string;
}
