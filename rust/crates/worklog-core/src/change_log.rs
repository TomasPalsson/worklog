//! The change log — detects and records automatic changes to a block's
//! customer, deild, split or description by diffing against the block's
//! last-seen `deild_contract::ResolutionSnapshot`, and serves the live
//! pop-up + catch-up feed (spec 006). Types live in `deild_contract`; see
//! `deild_contract::BlockChange`. Populated by T010: `new_batch`,
//! `refresh_day`, `feed`, `unseen`, `mark_seen`, `purge_old`.
