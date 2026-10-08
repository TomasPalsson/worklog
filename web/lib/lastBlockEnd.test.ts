import { describe, expect, it } from "bun:test";
import { lastBlockEnd } from "./lastBlockEnd";

const b = (ended_at: string, ignored_at: string | null = null) => ({ ended_at, ignored_at });

describe("lastBlockEnd", () => {
  it("is null for no blocks (wrong version: returns '' or throws)", () => {
    expect(lastBlockEnd([])).toBeNull();
  });

  it("is null when every block is ignored (wrong version: ignores the flag)", () => {
    expect(lastBlockEnd([b("2026-07-25T10:00:00Z", "2026-07-25T11:00:00Z")])).toBeNull();
  });

  it("skips an ignored block even when it ends latest", () => {
    const blocks = [b("2026-07-25T09:00:00Z"), b("2026-07-25T12:00:00Z", "x")];
    expect(lastBlockEnd(blocks)).toBe("2026-07-25T09:00:00Z");
  });

  it("picks the latest regardless of input order (wrong version: last element)", () => {
    const blocks = [b("2026-07-25T12:00:00Z"), b("2026-07-25T09:00:00Z")];
    expect(lastBlockEnd(blocks)).toBe("2026-07-25T12:00:00Z");
  });

  it("compares instants, not strings, across offsets (wrong version: string >)", () => {
    // 11:00+02:00 is 09:00Z, earlier than 10:00Z although it sorts later as text.
    const blocks = [b("2026-07-25T10:00:00Z"), b("2026-07-25T11:00:00+02:00")];
    expect(lastBlockEnd(blocks)).toBe("2026-07-25T10:00:00Z");
  });

  it("keeps the first of two equal instants (wrong version: >= swaps)", () => {
    const blocks = [b("2026-07-25T10:00:00Z"), b("2026-07-25T12:00:00+02:00")];
    expect(lastBlockEnd(blocks)).toBe("2026-07-25T10:00:00Z");
  });
});
