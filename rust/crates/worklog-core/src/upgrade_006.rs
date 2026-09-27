//! One-off upgrade for spec 006: deletes stored personal-owner commit/PR
//! events (FR-02) and re-infers every stored day once under the new
//! attribution rules, carrying `exported_at`, Tempo ids, manual
//! descriptions and tickets (FR-10, D-09). Populated by T005.
