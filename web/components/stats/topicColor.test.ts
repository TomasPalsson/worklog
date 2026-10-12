import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { topicColor, topicHue } from "./topicColor";

const angular = (a: number, b: number) => {
  const d = Math.abs(a - b) % 360;
  return Math.min(d, 360 - d);
};

describe("topicColor", () => {
  // golden-angle formula replaced the old 360/k spacing; these pins moved with it
  test.each([
    [0, 25],
    [1, 162.508],
    [2, 300.016],
    [3, 77.524],
  ])("golden-angle hue for id %i is %f", (id, hue) => {
    expect(topicHue(id, 10)).toBeCloseTo(hue, 3);
  });

  test("same id gives the same colour whatever k is", () => {
    expect(topicColor(7, 3)).toBe(topicColor(7, 20));
    expect(topicHue(7, 0)).toBe(topicHue(7, 20));
  });

  test("first 20 ids differ by at least 8 degrees of hue or by lightness step", () => {
    for (let a = 0; a < 20; a++)
      for (let b = a + 1; b < 20; b++)
        expect(angular(topicHue(a, 20), topicHue(b, 20)) >= 8 || a % 3 !== b % 3).toBe(true);
  });

  test("hue stays in [0, 360) for 20 ids", () => {
    for (let id = 0; id < 20; id++) {
      expect(topicHue(id, 20)).toBeGreaterThanOrEqual(0);
      expect(topicHue(id, 20)).toBeLessThan(360);
    }
  });

  test("lightness steps by id % 3: 0 none, 1 darker 0.08, 2 lighter 0.06", () => {
    expect(topicColor(3, 10)).toContain("var(--project-l) + 0)");
    expect(topicColor(4, 10)).toContain("var(--project-l) + -0.08)");
    expect(topicColor(5, 10)).toContain("var(--project-l) + 0.06)");
  });

  test("string keeps the oklch calc form with the hue last", () => {
    expect(topicColor(1, 10)).toBe(`oklch(calc(var(--project-l) + -0.08) var(--project-c) ${topicHue(1, 10)})`);
  });

  test("k of 0 does not divide by zero", () => {
    expect(Number.isFinite(topicHue(1, 0))).toBe(true);
  });

  test.each(["TopicGalaxy", "TopicHeartbeats", "TopicRiver", "DayMirror", "DayCompass", "TopicMoments"])(
    "%s defines no hue helper of its own",
    (name) => {
      const src = readFileSync(`${import.meta.dir}/${name}.tsx`, "utf8");
      expect(src).not.toMatch(/function topicHue|const topicHue|const hue =|const colorOf|360 \//);
    },
  );
});
