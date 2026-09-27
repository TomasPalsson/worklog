// Mirrors rust/crates/worklog-core/src/clues_contract.rs (spec 006). Kept
// in its own module rather than web/lib/types.ts, which is already over
// the repo's 400-line file guard — see T001-brief.md.

/** Mirrors `clues_contract::SECRET_PLACEHOLDER`. */
export const SECRET_PLACEHOLDER = "[secret]";
/** Mirrors `clues_contract::TOOL_OUTPUT_CAP_BYTES`. */
export const TOOL_OUTPUT_CAP_BYTES = 2048;
/** Mirrors `clues_contract::SOURCE_CLAUDE_HELPER`. */
export const SOURCE_CLAUDE_HELPER = "claude_helper";
/** Mirrors `clues_contract::SOURCE_CLAUDE_MESSAGE`. */
export const SOURCE_CLAUDE_MESSAGE = "claude_message";
/** Mirrors `clues_contract::SOURCE_CLAUDE_TOOL`. */
export const SOURCE_CLAUDE_TOOL = "claude_tool";

/** Mirrors `clues_contract::HelperKind`. */
export type HelperKind = "subagent" | "sidechain" | "background_job" | "teammate";

/** Mirrors `clues_contract::RawRecord`, the one shape stored in
 * `events.raw_json` from spec 006 on. */
export type RawRecord =
  | { kind: "shell"; command: string; cwd: string | null }
  | { kind: "reflog"; message: string }
  | { kind: "claude_prompt"; session_id: string; text: string }
  | {
      kind: "claude_tool";
      session_id: string;
      tool: string;
      input: unknown;
      output: string | null;
      /** Bytes cut from `output` by `TOOL_OUTPUT_CAP_BYTES`; 0 = whole. */
      output_cut_bytes: number;
      files: string[];
    }
  | {
      kind: "helper";
      parent_session_id: string;
      helper_kind: HelperKind;
      summary: string;
    }
  | { kind: "session_message"; from: string; text: string }
  | {
      kind: "commit";
      sha: string;
      body: string;
      /** Local work folder whose clone holds `sha`; `null` = elsewhere. */
      local_folder: string | null;
    }
  | { kind: "hook"; event: string; payload: unknown };

/** Mirrors `clues_contract::DescriptionInput` — everything the description
 * writer may see (D-02). */
export interface DescriptionInput {
  day: string;
  minutes: number;
  folder: string | null;
  branches: string[];
  /** Commit and PR first lines with PR/ticket numbers removed (D-13). */
  change_titles: string[];
  jira_key: string | null;
  candidate_ticket_titles: string[];
  file_basenames: string[];
  programs: string[];
  web_domains: string[];
  slack_channels: string[];
  /** Existing per-block descriptions feeding a billing-line text (D-12). */
  block_descriptions: string[];
}

/** Mirrors `clues_contract::BillingLineKey` — primary key of one billing
 * line (one invoice-form submission). */
export interface BillingLineKey {
  day: string;
  folder: string;
  /** `""` when the customer is unresolved. */
  customer: string;
}

/** Mirrors `clues_contract::LineTextOrigin`. */
export type LineTextOrigin = "generated" | "manual";
