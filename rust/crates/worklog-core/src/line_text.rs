//! Billing-line texts: one generated Icelandic text per billing line
//! (spec 006, D-12), hand edits that generation never overwrites (FR-31),
//! and the previous-text fallback on writer failure (FR-35). Populated by
//! T020: `validate`, `generate_for_day`, `set_manual`, `text_for`.
