//! Turns Firefox add-on heartbeats into `events` rows.
//!
//! Filters incognito tabs, the `PERSONAL_CONTAINER` container, and time
//! outside the configured work-hours window, then upserts one event per
//! heartbeat minute. See spec 003 T002.
