//! Secret scrubbing — replaces detected tokens, keys, passwords and other
//! secrets with `clues_contract::SECRET_PLACEHOLDER` before a value is
//! stored or sent off-machine (spec 006, D-03). Populated by T007:
//! `scrub_secrets`.
