//! The deildir registry — a per-customer list of deildir (Verkefni names)
//! with keywords, plus keyword matching against a block's ticket summary
//! and description (spec 006). Types live in `deild_contract`; see
//! `deild_contract::Deild`. Populated by T002: `list_deildir`,
//! `upsert_deild`, `delete_deild`, `deild_in_text`.
