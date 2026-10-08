//! Shared types for note blocks (spec 019): a block the Owner adds from
//! the day page's "+" with a rough note, which the AI then rewrites into
//! the block's description. Owned by the spec; task code imports from
//! here and never redeclares. Mirrored by `web/lib/noteBlock.ts`.

use serde::{Deserialize, Serialize};

/// Model and thinking budget for the note writer.
pub const NOTE_MODEL: &str = "claude-sonnet-5";
pub const NOTE_THINKING_TOKENS: u32 = 2000;

/// Job failure reasons reported by `GET /blocks/:id/note/status`.
pub const REASON_HAND_EDITED: &str = "hand-edited";
pub const REASON_NOT_A_NOTE_BLOCK: &str = "not a note block";

/// Who wrote a block's current description. Stored in
/// `blocks.description_origin`; NULL for every block that did not come
/// from the "+" form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DescriptionOrigin {
    /// Still the Owner's raw note — the AI has not written yet.
    Note,
    /// Written by the AI from the note.
    Ai,
    /// Hand-edited by the Owner; the AI never overwrites it unless forced.
    Hand,
}

impl DescriptionOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            DescriptionOrigin::Note => "note",
            DescriptionOrigin::Ai => "ai",
            DescriptionOrigin::Hand => "hand",
        }
    }

    pub fn parse(s: &str) -> Option<DescriptionOrigin> {
        match s {
            "note" => Some(DescriptionOrigin::Note),
            "ai" => Some(DescriptionOrigin::Ai),
            "hand" => Some(DescriptionOrigin::Hand),
            _ => None,
        }
    }
}

/// `POST /blocks/note` body. `day` is YYYY-MM-DD, `start` is local HH:MM.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteBlockBody {
    pub jira_issue: String,
    pub day: String,
    pub start: String,
    pub minutes: i64,
    pub note: String,
}

/// `POST /blocks/:id/note/regenerate` body. `force` overwrites a
/// hand-written description (the Owner confirmed "Replace your edit?").
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RegenerateNoteBody {
    #[serde(default)]
    pub force: bool,
}
