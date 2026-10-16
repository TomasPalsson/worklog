//! Shared types for "Regenerate with prompt" (spec 026): the Owner
//! right-clicks a block, types a one-time instruction, and the AI rewrites
//! ONLY that block's description. Owned by the spec; task code imports
//! from here and never redeclares. Mirrored by `web/lib/redescribe.ts`.

use serde::{Deserialize, Serialize};

/// Longest instruction the Owner may send, in chars, after trimming.
pub const MAX_INSTRUCTION_CHARS: usize = 500;

/// 400 bodies for `POST /blocks/:id/redescribe`.
pub const REASON_EMPTY_INSTRUCTION: &str = "instruction is empty";
pub const REASON_INSTRUCTION_TOO_LONG: &str = "instruction is longer than 500 characters";
pub const REASON_PERSONAL: &str = "personal blocks are not described";
pub const REASON_NOT_DESCRIBED: &str = "block has no description yet — describe it first";
pub const REASON_NOTE_BLOCK: &str = "note blocks are rewritten from their note";

/// `POST /blocks/:id/redescribe` body.
#[derive(Debug, Clone, Deserialize)]
pub struct RedescribeBody {
    pub instruction: String,
}

/// `POST /blocks/:id/redescribe` 200 body. Only the description changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedescribeResult {
    pub block_id: i64,
    pub description: String,
}
