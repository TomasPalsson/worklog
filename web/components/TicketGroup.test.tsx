// The "needs a look" badge shows only for a generated line Verdict flagged
// needs_look; passed/null checks and the Owner's own text never show it.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { BlockGroup } from "@/app/[day]/page";
import type { TempoLine } from "@/lib/tempo_line_contract";
import type { LineCheck } from "@/lib/verdict_contract";

mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: mock(() => {}) }),
  usePathname: () => "/",
}));

let TicketGroup: typeof import("./TicketGroup").TicketGroup;

beforeAll(async () => {
  TicketGroup = (await import("./TicketGroup")).TicketGroup;
});

afterEach(cleanup);

const group: BlockGroup = {
  key: "ABC-1",
  label: "ABC-1",
  unassigned: false,
  blocks: [],
  totalSeconds: 3600,
  syncState: "unsynced",
  previewDescription: "work",
  defaultOpen: true,
};

function renderLine(origin: TempoLine["text_origin"], check: LineCheck | null | undefined) {
  const line: TempoLine & { check_status?: LineCheck | null } = {
    day: "2026-10-06",
    jira_issue: "ABC-1",
    text: "did stuff",
    text_origin: origin,
    fallback_text: "fallback",
    union_seconds: 3600,
    hours_override_seconds: null,
    effective_seconds: 3600,
    check_status: check,
  };
  render(
    <TicketGroup group={group} day="2026-10-06" line={line}>
      <div />
    </TicketGroup>,
  );
}

describe("needs-a-look badge", () => {
  it("shows for needs_look on generated text", () => {
    renderLine("generated", "needs_look"); // catches: badge condition dropped
    expect(screen.getByText("needs a look")).toBeTruthy();
  });

  it("is hidden for passed", () => {
    renderLine("generated", "passed"); // catches: showing on any check_status
    expect(screen.queryByText("needs a look")).toBeNull();
  });

  it("is hidden for null and missing check_status", () => {
    renderLine("generated", null); // catches: inverted condition
    expect(screen.queryByText("needs a look")).toBeNull();
    cleanup();
    renderLine("generated", undefined);
    expect(screen.queryByText("needs a look")).toBeNull();
  });

  it("is hidden when the text is the Owner's own", () => {
    renderLine("manual", "needs_look"); // catches: generated guard removed
    expect(screen.queryByText("needs a look")).toBeNull();
  });
});
