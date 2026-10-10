import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { PromptStats } from "@/lib/stats_contract";
import { ARM_L, ARM_R, BODY, HEAD, antennaColor, bubbleSpecs, headline, layoutBubbles, moodOf, niceStep, PoliteBot, splitLines, tapeShare } from "./PoliteBot";

afterEach(cleanup);

const base: PromptStats = {
  count: 500,
  avg_chars: 120,
  longest_chars: 4321,
  questions: 168,
  please: 102,
  thanks: 0,
  sorry: 14,
  swears: 28,
  exclaims: 10,
  interrupts: 159,
  slash_commands: 103,
  top_openers: [
    { label: "fix", value: 40 },
    { label: "add", value: 10 },
  ],
};
const p = (o: Partial<PromptStats>): PromptStats => ({ ...base, ...o });
const quiet = p({ please: 0, thanks: 0, sorry: 0, swears: 0, interrupts: 0 });

describe("moodOf", () => {
  it("no signal is neutral", () => expect(moodOf(quiet)).toBe("neutral"));
  it("walks the five moods", () => {
    expect(moodOf(p({ please: 100, thanks: 0, sorry: 0, swears: 5, interrupts: 0 }))).toBe("delighted");
    expect(moodOf(base)).toBe("happy");
    expect(moodOf(p({ please: 10, thanks: 0, sorry: 0, swears: 10, interrupts: 0 }))).toBe("neutral");
    expect(moodOf(p({ please: 3, thanks: 0, sorry: 0, swears: 10, interrupts: 0 }))).toBe("wary");
    expect(moodOf(p({ please: 0, thanks: 0, sorry: 0, swears: 10, interrupts: 0 }))).toBe("grumpy");
  });
  it("interrupts count a fifth", () => {
    expect(moodOf(p({ please: 1, thanks: 0, sorry: 0, swears: 0, interrupts: 500 }))).toBe("grumpy");
  });
  it("antenna colours", () => {
    expect(antennaColor("happy")).toBe("var(--sage)");
    expect(antennaColor("wary")).toBe("var(--amber)");
    expect(antennaColor("grumpy")).toBe("var(--terracotta)");
  });
});

describe("headline", () => {
  it("quotes the counts", () => expect(headline(base)).toBe("Mostly polite: 116 kind words vs 28 swears · cut off 159×"));
  it("quotes swears when they outweigh interrupts/5", () =>
    expect(headline(p({ ...base, interrupts: 10 }))).toBe("Mostly polite: 116 kind words vs 28 swears · cut off 10×"));
  it("empty", () => expect(headline(p({ count: 0 }))).toBe("Say hello to Claude"));
});

describe("bubbles", () => {
  it("drops zero counts and quotes the real ones", () => {
    const t = bubbleSpecs(base).map((b) => b.text);
    expect(t).toContain("please ×102");
    expect(t).toContain("cut me off ×159");
    expect(t).toContain("103 slash commands");
    expect(t.some((x) => x.startsWith("thanks"))).toBe(false);
  });
  it("splits long labels", () => {
    expect(splitLines("168 questions")).toEqual(["168 questions"]);
    expect(splitLines("103 slash commands")).toEqual(["103 slash", "commands"]);
  });
  it("never overlaps, stays in its column, bigger count is bigger", () => {
    const { placed, height } = layoutBubbles(bubbleSpecs(base));
    for (const side of ["L", "R"]) {
      const m = placed.filter((b) => b.side === side).sort((a, b) => a.y - b.y);
      m.forEach((b, i) => {
        expect(b.y).toBeGreaterThanOrEqual(0);
        expect(b.y + b.h).toBeLessThanOrEqual(height + 0.001);
        if (i > 0) expect(b.y).toBeGreaterThanOrEqual(m[i - 1].y + m[i - 1].h);
      });
    }
    expect(placed.every((b) => b.x >= 0 && b.x + b.w <= 336)).toBe(true);
    const big = placed.find((b) => b.key === "interrupts")!;
    const small = placed.find((b) => b.key === "sorry")!;
    expect(big.font).toBeGreaterThan(small.font);
  });
  it("huge values stay finite and inside the column", () => {
    const { placed } = layoutBubbles(bubbleSpecs(p({ please: 1e9, swears: 1 })));
    expect(placed.every((b) => Number.isFinite(b.w) && b.w <= 108)).toBe(true);
  });
});

describe("tape maths", () => {
  it("niceStep", () => {
    expect(niceStep(0)).toBe(0);
    expect(niceStep(4321)).toBe(1000);
    expect(niceStep(100)).toBe(20);
    expect(niceStep(1)).toBe(0.2);
  });
  it("share is clamped", () => {
    expect(tapeShare(p({ avg_chars: 5, longest_chars: 0 }))).toBe(0);
    expect(tapeShare(p({ avg_chars: 50, longest_chars: 10 }))).toBe(1);
  });
});

describe("PoliteBot", () => {
  it("draws with an aria label and tooltips", () => {
    const { container } = render(<PoliteBot prompt={base} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("role")).toBe("img");
    expect(svg.getAttribute("aria-label")).toContain("102 please");
    expect(svg.getAttribute("aria-label")).toContain("looks happy");
    expect(container.querySelectorAll("[data-stip]").length).toBeGreaterThan(6);
    expect(container.textContent).toContain("Mostly polite: 116 kind words vs 28 swears · cut off 159×");
    expect(container.textContent).toContain("fix");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("empty state", () => {
    const { container } = render(<PoliteBot prompt={p({ count: 0 })} />);
    expect(container.textContent).toContain("nothing built yet");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("single bubble, zero length, no openers", () => {
    const { container } = render(
      <PoliteBot prompt={p({ ...quiet, please: 1, questions: 0, slash_commands: 0, avg_chars: 0, longest_chars: 0, top_openers: [] })} />,
    );
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.querySelector("ul")).toBeNull();
  });
  it("all-zero counts with prompts", () => {
    const { container } = render(<PoliteBot prompt={p({ ...quiet, questions: 0, slash_commands: 0 })} />);
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.querySelector("svg")!.getAttribute("aria-label")).toContain("looks neutral");
  });
});

describe("headline wording", () => {
  it("quotes interruptions when they drive the mood", () =>
    expect(headline(p({ please: 10, thanks: 0, sorry: 0, swears: 0, interrupts: 200 }))).toBe("A bit curt: 10 kind words vs 0 swears · cut off 200×"));
  it("singular", () =>
    expect(headline(p({ please: 1, thanks: 0, sorry: 0, swears: 1, interrupts: 0 }))).toBe("Evenly matched: 1 kind word vs 1 swear"));
});

describe("arms and tape", () => {
  type B = { x: number; y: number; z: number; w: number; d: number; h: number };
  // Viewer looks along (+1,+1,+1); a point is hidden if its ray enters a box.
  const hit = (b: B, x: number, y: number, z: number) => {
    let lo = 0;
    let hi = 1e9;
    for (const [o, a, s] of [[x, b.x, b.w], [y, b.y, b.d], [z, b.z, b.h]] as const) {
      lo = Math.max(lo, a - o);
      hi = Math.min(hi, a + s - o);
    }
    return lo < hi && hi > 1e-6;
  };
  const visible = (arm: B, lift: number) => {
    let n = 0;
    for (let i = 0; i < 8; i++)
      for (let j = 0; j < 8; j++)
        for (let k = 0; k < 8; k++) {
          const x = arm.x + ((i + 0.5) / 8) * arm.w;
          const y = arm.y + ((j + 0.5) / 8) * arm.d;
          const z = arm.z + lift + ((k + 0.5) / 8) * arm.h;
          if (!hit(BODY, x, y, z) && !hit(HEAD, x, y, z)) n++;
        }
    return n;
  };
  it("both arms show, lifted or not", () => {
    for (const lift of [0, 0.8]) for (const a of [ARM_L, ARM_R]) expect(visible(a, lift)).toBeGreaterThan(0);
  });
  it("prompt length caption sits below the avg label row", () => {
    const { container } = render(<PoliteBot prompt={base} />);
    const ts = [...container.querySelectorAll("text")];
    const t = ts.find((e) => e.textContent === "prompt length")!;
    const avg = ts.find((e) => e.textContent?.startsWith("avg "))!;
    expect(Number(t.getAttribute("y"))).toBeGreaterThan(Number(avg.getAttribute("y")));
  });
});

import { botTip, bubbleTip, openerTip, tapeTip } from "./PoliteBotTips";
import { parseTip } from "./tip";

describe("bot tips", () => {
  const specs = bubbleSpecs(base);
  const spec = (k: string) => specs.find((s) => s.key === k)!;
  it("please bubble", () => {
    const t = bubbleTip(spec("please"), specs, base, 10);
    expect(t.title).toBe("please ×102");
    expect(t.sub).toBe("#4 of 6 bubbles");
    expect(t.rows).toEqual([
      ["Prompts containing it", "102"],
      ["Share of prompts", "20%"],
      ["Per worked day", "10.2"],
      ["What counts", 'prompts with the word "please"'],
    ]);
    expect(t.bar).toEqual({ value: 102, max: 500, label: "vs 500 prompts" });
    expect(t.note).toBe("About 1 prompt in 5 has this.");
    expect(t.accent).toBe("var(--sage)");
    expect(bubbleTip(spec("please"), specs, base).rows!.length).toBe(3);
  });
  it("swears and interrupts bubbles", () => {
    expect(bubbleTip(spec("swears"), specs, base).note).toBe("4.1 kind words for every swear.");
    const i = bubbleTip(spec("interrupts"), specs, base);
    expect(i.rows![0]).toEqual(["Turns", "159"]);
    expect(i.rows![1]).toEqual(["Per 100 prompts", "31.8"]);
    expect(i.note).toBe("That's 0.32 per prompt.");
    const z = bubbleTip(spec("interrupts"), specs, p({ count: 0 }));
    expect(z.rows![1]).toEqual(["Per 100 prompts", "0"]);
    expect(JSON.stringify(z)).not.toContain("NaN");
  });
  it("bot tip explains the mood", () => {
    const t = botTip(base, "happy");
    expect(t.title).toBe("Politeness bot: happy");
    expect(t.sub).toBe("Mostly polite");
    expect(t.rows).toEqual([
      ["Kind words", "116 (please + thanks + sorry)"],
      ["Swears", "28"],
      ["Interruptions", "159 (5 count as 1)"],
      ["Politeness score", "116"],
      ["Rudeness score", "59.8"],
      ["Kindness share", "66%"],
    ]);
    expect(botTip(quiet, "neutral").rows![5]).toEqual(["Kindness share", "no signal yet"]);
  });
  it("tape tip", () => {
    const t = tapeTip(base);
    expect(t.rows).toEqual([["Average", "120 chars"], ["Longest", "4,321 chars"], ["Average vs longest", "3%"]]);
    expect(t.note).toBe("Your longest prompt was 36× the average.");
    expect(tapeTip(p({ avg_chars: 0, longest_chars: 0 })).note).toBe("Your longest prompt is barely longer than the average.");
  });
  it("opener tips", () => {
    const a = openerTip(base.top_openers[0], base.top_openers, base, 10);
    expect(a.title).toBe('"fix"');
    expect(a.sub).toBe("your go-to first word, #1 of 2");
    expect(a.rows).toEqual([["Prompts starting with it", "40"], ["Share of prompts", "8%"], ["Per worked day", "4.0"], ["Top openers together", "10%"]]);
    expect(a.note).toBe('8% of your prompts start with "fix".');
    const b = openerTip(base.top_openers[1], base.top_openers, base);
    expect(b.sub).toBe("#2 of 2 openers");
    expect(b.note).toBe('"fix" beats it by 30 prompts.');
  });
  it("marks carry data-stip, no native <title>, keyboard reachable", () => {
    const { container } = render(<PoliteBot prompt={base} daysWorked={10} />);
    expect(container.querySelector("title")).toBeNull();
    expect(container.querySelector("li[title]")).toBeNull();
    const tipped = [...container.querySelectorAll("[data-stip]")];
    expect(tipped.length).toBe(10); // bot + 6 bubbles + tape + 2 openers
    expect(tipped.every((e) => e.getAttribute("tabindex") === "0")).toBe(true);
    const titles = tipped.map((e) => parseTip(e.getAttribute("data-stip"))!.title);
    expect(titles).toContain("please ×102");
    expect(titles).toContain("Politeness bot: happy");
    expect(titles).toContain("Prompt length");
    expect(titles).toContain('"fix"');
    expect(container.innerHTML).not.toContain("NaN");
  });
});
