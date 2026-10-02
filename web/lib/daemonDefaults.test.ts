import { expect, it } from "bun:test";

import { withBlocksDefaults, withDetailDefaults } from "./daemonDefaults";

it("an older daemon's ticket detail gets empty values for the fields it doesn't send", () => {
  const d = withDetailDefaults({ key: "A-1", summary: "S", url: "u", comments: [{ id: "1", author: "a", created: "c", body: "b" }] });
  expect(d.subtasks).toEqual([]);
  expect(d.links).toEqual([]);
  expect(d.attachments).toEqual([]);
  expect(d.time_spent_seconds).toBeNull();
  expect(d.comments).toHaveLength(1); // sent fields win
});

it("an older daemon's work log gets unknown Tempo totals and an empty today", () => {
  const b = withBlocksDefaults({ key: "A-1", from: "2026-09-19", to: "2026-10-02", days: [] });
  expect(b.in_tempo_total_seconds).toBeNull();
  expect(b.pulled_at).toBeNull();
  expect(b.today).toEqual({ day: "2026-10-02", worked_seconds: 0, in_tempo_seconds: null, ticket_worked_seconds: 0, ticket_in_tempo_seconds: null });
});
