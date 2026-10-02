// Fields an older daemon doesn't send yet come back empty, so a web-ahead-of-daemon install never crashes.

import type { TicketBlocks, TicketDetail } from "./types";

export function withDetailDefaults(d: Partial<TicketDetail> & Pick<TicketDetail, "key" | "summary" | "url">): TicketDetail {
  return {
    status: null, status_category: null, issue_type: null, priority: null, assignee: null, updated: null,
    description: "", comments: [], reporter: null, created: null, labels: [], due_date: null, components: [],
    fix_versions: [], time_spent_seconds: null, original_estimate_seconds: null, remaining_estimate_seconds: null,
    parent: null, subtasks: [], links: [], attachments: [], ...d,
  };
}

export function withBlocksDefaults(b: Partial<TicketBlocks> & Pick<TicketBlocks, "key" | "from" | "to" | "days">): TicketBlocks {
  const today = { day: b.to, worked_seconds: 0, in_tempo_seconds: null, ticket_worked_seconds: 0, ticket_in_tempo_seconds: null };
  return { in_tempo_total_seconds: null, pulled_at: null, today, ...b };
}
