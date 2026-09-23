//! Client for the optional Laya classifier helper.
//!
//! Laya runs as a local `uv run --with laya` process on `LAYA_ADDR`; a
//! connection failure is treated as "no guess available", not an error.
//! See spec 003 T006.
