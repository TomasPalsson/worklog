//! Routes browser/Slack `events` rows to a project folder.
//!
//! A hard rule wins over a model guess; guesses below the confidence
//! threshold leave the event unsorted. See spec 003 T005.
