import { expect, it } from "bun:test";

import { barModel, chartModel, initials, pendingSeconds, toneFor } from "./progress";
import type { PersonHours, TicketProgress } from "./types";

const H = 3600;
const person = (name: string, seconds: number, is_you = false, by_day: Array<[string, number]> = []): PersonHours => ({
  account_id: name,
  name,
  is_you,
  seconds,
  by_day,
});
const ticket = (people: PersonHours[], estimate: number | null = 4 * H): TicketProgress => ({
  key: "GENAI-1897",
  estimate_seconds: estimate,
  people,
  logged_seconds: people.reduce((a, p) => a + p.seconds, 0),
  pulled_at: null,
  error: null,
});

it("B4: You, Jón Geir, pending 1h -> used 5h30m, over by 1h30m", () => {
  const m = barModel(ticket([person("Tomas", 2 * H, true), person("Jón Geir", 2.5 * H)]), H)!;
  expect(m.segments.map((s) => [s.name, s.seconds])).toEqual([["Tomas", 2 * H], ["Jón Geir", 2.5 * H]]);
  expect(m.pending).toBe(H);
  expect(m.used).toBe(5.5 * H);
  expect(m.tone).toBe("over");
  expect(m.over).toBe(1.5 * H);
  expect(m.left).toBe(0);
});

it("FR-06: You first even when a teammate has more hours; teammates by hours desc", () => {
  // catches: trusting input order / sorting by seconds only
  const m = barModel(ticket([person("B", 2 * H), person("Me", H, true), person("A", 3 * H)]), 0)!;
  expect(m.segments.map((s) => s.name)).toEqual(["Me", "A", "B"]);
});

it("B6/FR-06a: exactly 4 people stay named; 5 and 6 fold into Others", () => {
  // catches: > vs >= off-by-one in the fold threshold
  const four = barModel(ticket([person("Me", H, true), person("A", 4), person("B", 3), person("C", 2)]), 0)!;
  expect(four.segments.map((s) => s.name)).toEqual(["Me", "A", "B", "C"]);
  const six = barModel(
    ticket([person("Me", H, true), person("A", 50), person("B", 40), person("C", 30), person("D", 20), person("E", 10)]),
    0,
  )!;
  expect(six.segments.map((s) => [s.name, s.seconds])).toEqual([["Me", H], ["A", 50], ["B", 40], ["C", 30], ["Others", 30]]);
  const five = barModel(ticket([person("Me", H, true), person("A", 4), person("B", 3), person("C", 2), person("D", 1)]), 0)!;
  expect(five.segments.map((s) => [s.name, s.seconds]).pop()).toEqual(["Others", 1]);
});

it("FR-07: pending 0 adds nothing to used", () => {
  const m = barModel(ticket([person("Me", H, true)]), 0)!;
  expect([m.pending, m.used]).toEqual([0, H]);
});

it("FR-09: tone at 79%, 80%, 100%, 101%", () => {
  // catches: < vs <= at 80 and 100; float rounding
  expect(toneFor(79, 100)).toBe("ok");
  expect(toneFor(80, 100)).toBe("low");
  expect(toneFor(100, 100)).toBe("low");
  expect(toneFor(101, 100)).toBe("over");
});

it("FR-09: exactly at estimate has nothing left and nothing over", () => {
  const m = barModel(ticket([person("Me", 4 * H, true)]), 0)!;
  expect([m.tone, m.left, m.over]).toEqual(["low", 0, 0]);
  const under = barModel(ticket([person("Me", 3 * H, true)]), 0)!;
  expect([under.left, under.over]).toEqual([H, 0]);
});

it("FR-14: null or 0 estimate gives no model", () => {
  // catches: dividing by zero / treating 0 as an estimate
  expect(barModel(ticket([person("Me", H, true)], null), 0)).toBeNull();
  expect(barModel(ticket([person("Me", H, true)], 0), 0)).toBeNull();
});

it("A5: initials are first letters of first two name parts", () => {
  expect(initials("Jón Geir Sigurðsson")).toBe("JG");
  expect(initials("madonna")).toBe("M");
  expect(initials("  ")).toBe("");
});

it("A3: pending only when tempo id is empty or null", () => {
  expect(pendingSeconds({ tempo_worklog_id: null, duration_seconds: 60 })).toBe(60);
  expect(pendingSeconds({ tempo_worklog_id: "", duration_seconds: 60 })).toBe(60);
  expect(pendingSeconds({ tempo_worklog_id: "123", duration_seconds: 60 })).toBe(0);
});

const day = (n: number) => `2026-09-${String(n).padStart(2, "0")}`;

it("FR-11a: 20 logged days -> 14 points, older hours folded into the first running total", () => {
  // catches: slicing without folding; a running total started from zero at the window
  const days = Array.from({ length: 20 }, (_, i) => day(i + 1));
  const me = person("Me", 20 * H, true, days.map((d): [string, number] => [d, H]));
  const c = chartModel(ticket([me]))!;
  expect(c.days).toEqual(days.slice(6));
  expect(c.series[0].values[0]).toBe(7 * H);
  expect(c.series[0].values[13]).toBe(20 * H);
  expect(c.estimate).toBe(4 * H);
});

it("FR-11a: exactly 14 logged days are not folded", () => {
  const days = Array.from({ length: 14 }, (_, i) => day(i + 1));
  const c = chartModel(ticket([person("Me", 14 * H, true, days.map((d): [string, number] => [d, H]))]))!;
  expect(c.days).toEqual(days);
  expect(c.series[0].values[0]).toBe(H);
});

it("FR-11: days are the union across people; a person's total carries over days they skipped", () => {
  const c = chartModel(
    ticket([person("Me", 3 * H, true, [[day(1), H], [day(3), 2 * H]]), person("B", H, false, [[day(2), H]])]),
  )!;
  expect(c.days).toEqual([day(1), day(2), day(3)]);
  expect(c.series.map((s) => s.values)).toEqual([[H, H, 3 * H], [0, H, H]]);
});

it("FR-11: people beyond 4 fold into an Others series; no estimate or no logs gives no chart", () => {
  const ps = [person("Me", H, true, [[day(1), H]])];
  for (const n of ["A", "B", "C", "D", "E"]) ps.push(person(n, 1, false, [[day(1), 1]]));
  const c = chartModel(ticket(ps))!;
  expect(c.series.map((s) => s.name)).toEqual(["Me", "A", "B", "C", "Others"]);
  expect(c.series[4].values).toEqual([2]);
  expect(chartModel(ticket(ps, null))).toBeNull();
  expect(chartModel(ticket([person("Me", 0, true)]))).toBeNull();
});
