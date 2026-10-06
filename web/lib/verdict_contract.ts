// Shared shapes for spec 017 (Verdict does more). Mirrors
// rust/crates/worklog-core/src/verdict_contract.rs; a field needed here
// that is missing is an escalation, never a local re-declaration.

export type VerdictState =
  | { state: "off" }
  | { state: "starting" }
  | { state: "running" }
  | { state: "not_answering" }
  | { state: "needs_uv" }
  | { state: "stopped"; error: string };

export type VerdictStatus = VerdictState & {
  unchecked: number;
  scorecard: string | null;
};

export type RankedOption = { id: string; probability: number };

export type LineCheck = "passed" | "needs_look";

export type ReviewStatus =
  | { status: "sent" }
  | { status: "not_sent"; error: string };

export type ReviewLine = ReviewStatus & {
  day: string;
  jira_issue: string;
  seconds: number;
  text: string;
};
