import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { topicColor, topicHue } from "./topicColor";

describe("topicColor", () => {
  test("hue for id 3 of 10 is 133", () => {
    expect(topicHue(3, 10)).toBe(133);
  });

  test("even ids carry no lightness offset", () => {
    expect(topicColor(2, 10)).toBe("oklch(calc(var(--project-l) + 0) var(--project-c) 97)");
  });

  test("odd ids are 0.08 darker", () => {
    expect(topicColor(3, 10)).toBe("oklch(calc(var(--project-l) + -0.08) var(--project-c) 133)");
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
