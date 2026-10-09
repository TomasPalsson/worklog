//! Prompt-habit counters for the stats page (see `stats.rs`).

use std::collections::HashMap;

use crate::stats::{bump, ranked};
use crate::stats_contract::PromptStats;

#[derive(Default)]
pub(crate) struct PromptAcc {
    stats: PromptStats,
    chars: i64,
    openers: HashMap<String, i64>,
}

impl PromptAcc {
    pub(crate) fn add(&mut self, text: &str) {
        let s = &mut self.stats;
        // Not typed words: Esc-interrupts, slash commands and other
        // harness wrappers ("<task-notification>", ...) get their own
        // counters (or none) instead of polluting the habit counters.
        let head = text.trim_start();
        if head.starts_with("[Request interrupted") {
            s.interrupts += 1;
            return;
        }
        if head.starts_with("<command-") {
            s.slash_commands += 1;
            return;
        }
        if head.starts_with('<') {
            return;
        }
        let n = text.chars().count() as i64;
        s.count += 1;
        self.chars += n;
        s.longest_chars = s.longest_chars.max(n);
        if text.trim_end().ends_with('?') {
            s.questions += 1;
        }
        if text.contains('!') {
            s.exclaims += 1;
        }
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let has = |f: &dyn Fn(&str) -> bool| words.iter().any(|w| f(w));
        s.please += i64::from(has(&|w| w == "please"));
        s.sorry += i64::from(has(&|w| w == "sorry"));
        s.swears += i64::from(has(&|w| {
            ["fuck", "shit", "damn", "wtf", "crap"]
                .iter()
                .any(|p| w.starts_with(p))
        }));
        let thank_you = words.windows(2).any(|p| p == ["thank", "you"]);
        s.thanks += i64::from(thank_you || has(&|w| matches!(w, "thanks" | "thx" | "ty")));
        if let Some(first) = text
            .split_whitespace()
            .next()
            .map(|w| {
                w.to_lowercase()
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string()
            })
            .filter(|w| !w.is_empty())
        {
            bump(&mut self.openers, first, 1);
        }
    }

    pub(crate) fn finish(mut self) -> PromptStats {
        if self.stats.count > 0 {
            self.stats.avg_chars = (self.chars as f64 / self.stats.count as f64).round() as i64;
        }
        self.stats.top_openers = ranked(self.openers, 8);
        self.stats
    }
}
