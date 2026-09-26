// Shared types for per-customer deildir, (customer, deild, %) splits and
// the change log (spec 006). Mirrors rust/crates/worklog-core/src/
// deild_contract.rs field-for-field; task code imports from here and never
// redeclares.

import type { SplitOrigin } from "@/lib/tenants";

/** Pop-up poll interval (§5: change → pop-up ≤ 15 s). */
export const LIVE_POLL_SECONDS = 10;
/** A split's percentages must total 100 within this (FR-05). */
export const SHARE_TOLERANCE_PERCENT = 0.1;

export interface Deild {
  id?: number | null;
  customer: string;
  name: string;
  keywords: string[];
}

export type DeildOrigin = "manual" | "keyword" | "folder_default" | "blank";

/** One split row; a customer may repeat with different deildir. */
export interface ShareRow {
  customer: string;
  deild: string | null;
  /** (0, 1]; a block's rows sum to 1. */
  fraction: number;
}

export interface BillingSlice {
  customer: string | null;
  deild: string | null;
  intervals: [number, number][];
  origin: SplitOrigin;
  deild_origin: DeildOrigin;
}

export type ChangeField = "customer" | "deild" | "split" | "description";

export type ChangeSource = "claude" | "verdict" | "keyword" | "rebuild" | "user";

export const CHANGE_SOURCE_LABELS: Record<ChangeSource, string> = {
  claude: "Claude",
  verdict: "Verdict",
  keyword: "Keyword guess",
  rebuild: "Block rebuild",
  user: "You",
};

export interface BlockChange {
  id: number;
  day: string;
  started_at: string;
  field: ChangeField;
  old: string | null;
  new: string | null;
  source: ChangeSource;
  batch: string;
  created_at: string;
  seen: boolean;
}

export interface ChangeBatch {
  batch: string;
  source: ChangeSource;
  count: number;
}

export interface ChangeFeed {
  changes: BlockChange[];
  batches: ChangeBatch[];
  cursor: number;
}
