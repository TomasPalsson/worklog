import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { LineHours } from "./LineHours";
import type { TempoLine } from "@/lib/tempo_line_contract";

afterEach(cleanup);

const key = { day: "2026-10-01", jira_issue: "ABC-1" };
const save = mock(async () => ({ ok: true as const, data: {} as TempoLine }));

function line(overrides: Partial<TempoLine>): TempoLine {
  return {
    ...key,
    text: null,
    text_origin: null,
    fallback_text: "",
    union_seconds: 5400,
    hours_override_seconds: null,
    effective_seconds: 5400,
    billing: null,
    ...overrides,
  };
}

const show = (l: TempoLine) => render(<LineHours label="ABC-1" line={l} lineKey={key} saveHours={save} />);

describe("LineHours", () => {
  it("shows effective_seconds as the billed hours", () => {
    show(line({ effective_seconds: 5400 }));
    expect(screen.getByRole("button", { name: /1\.5h billed/ })).toBeTruthy();
  });

  it("does not re-round the server's billed hours", () => {
    show(line({ effective_seconds: 899, union_seconds: 899 }));
    expect(screen.getByRole("button", { name: /0\.2h billed/ })).toBeTruthy();
  });

  it("shows union_seconds as tracked when hours were changed", () => {
    show(line({ hours_override_seconds: 7200, effective_seconds: 7200, union_seconds: 1800 }));
    expect(screen.getByText(/hours changed · 0\.5h tracked/)).toBeTruthy();
  });
});
