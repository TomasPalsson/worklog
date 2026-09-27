//! Claude transcript tool-call capture: builds `clues_contract::
//! RawRecord::ClaudeTool` from a transcript tool-use turn, applying
//! `scrub::scrub_secrets` and the `clues_contract::TOOL_OUTPUT_CAP_BYTES`
//! output cap (spec 006, FR-14, FR-15). Populated by T010.
