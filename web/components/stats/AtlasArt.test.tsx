import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { AtlasEmpty, FirstTimeMark, OddballMark, RhythmIcon } from "./AtlasArt";

afterEach(cleanup);

const HEX = /#[0-9a-fA-F]{3,8}\b/;

const cases = [
  ["RhythmIcon always", <RhythmIcon kind="always" />, "16"],
  ["RhythmIcon sprint", <RhythmIcon kind="sprint" />, "16"],
  ["RhythmIcon one_off", <RhythmIcon kind="one_off" />, "16"],
  ["FirstTimeMark", <FirstTimeMark />, "14"],
  ["OddballMark", <OddballMark />, "14"],
  ["AtlasEmpty helper_down", <AtlasEmpty kind="helper_down" />, "120"],
  ["AtlasEmpty too_few", <AtlasEmpty kind="too_few" />, "120"],
  ["AtlasEmpty no_map", <AtlasEmpty kind="no_map" />, "120"],
] as const;

describe("AtlasArt", () => {
  for (const [name, el, width] of cases) {
    it(`${name} renders an svg of the delivered size with no hex colours`, () => {
      const { container } = render(el);
      const svg = container.querySelector("svg");
      expect(svg).not.toBeNull();
      expect(svg!.getAttribute("width")).toBe(width);
      expect(container.innerHTML).not.toMatch(HEX);
    });
  }

  it("icons and marks are aria-hidden; they are paired with text", () => {
    for (const el of [<RhythmIcon kind="always" />, <FirstTimeMark />, <OddballMark />]) {
      const { container, unmount } = render(el);
      expect(container.querySelector("svg")!.getAttribute("aria-hidden")).toBe("true");
      unmount();
    }
  });

  it("rhythm kinds draw different art", () => {
    const html = (k: "always" | "sprint" | "one_off") => {
      const { container, unmount } = render(<RhythmIcon kind={k} />);
      const h = container.innerHTML;
      unmount();
      return h;
    };
    expect(new Set([html("always"), html("sprint"), html("one_off")]).size).toBe(3);
  });

  const alts = {
    helper_down: "A telescope parked pointing down with its lens cap on, facing away from a star",
    too_few: "A nearly empty star chart with only three faint stars",
    no_map: "A star chart with a few dots and a dotted outline where the map will appear",
  } as const;
  for (const [kind, alt] of Object.entries(alts)) {
    it(`AtlasEmpty ${kind} is role=img with its alt text`, () => {
      const { container } = render(<AtlasEmpty kind={kind as keyof typeof alts} />);
      const svg = container.querySelector("svg")!;
      expect(svg.getAttribute("role")).toBe("img");
      expect(svg.getAttribute("aria-label")).toBe(alt);
    });
  }
});
