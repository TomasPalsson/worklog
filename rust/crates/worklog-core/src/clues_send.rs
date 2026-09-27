//! The only producer of anything sent off-machine (spec 006, D-02): builds
//! a `clues_contract::DescriptionInput` from a block or a billing line,
//! scrubbed and stripped of every forbidden field. Populated by T018:
//! `build_block_input`, `build_line_input`.
