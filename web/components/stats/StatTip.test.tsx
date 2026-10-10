import { afterEach, describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { TipLayer } from "./StatTip";
import { parseTip, placeTip, tipProps } from "./tip";

afterEach(cleanup);

describe("StatTip", () => {
  it("uses its own attribute, not the app's plain data-tip tooltips", () => {
    // globals.css turns any [data-tip] into a CSS ::after label: sharing the
    // name drew every card's JSON as a 2,600px invisible label (page overflow).
    expect(Object.keys(tipProps({ title: "x" }))).toEqual(["data-stip"]);
  });

  it("round-trips a tip through the data attribute", () => {
    const t = { title: "Tue 22 Sep", rows: [["work", "11h 20m"]] as [string, string][] };
    expect(parseTip(tipProps(t)["data-stip"])).toEqual(t);
    expect(parseTip("not json")).toBeNull();
    expect(parseTip(null)).toBeNull();
    expect(parseTip('{"rows":[]}')).toBeNull();
  });

  it("keeps the card inside the viewport", () => {
    expect(placeTip(100, 100, 200, 80, 1000, 800)).toEqual({ left: 114, top: 114 });
    // flips left and up near the bottom-right corner
    expect(placeTip(950, 780, 200, 80, 1000, 800)).toEqual({ left: 736, top: 686 });
    // never off the top-left edge
    expect(placeTip(5, 5, 400, 900, 300, 300)).toEqual({ left: 8, top: 8 });
  });

  it("the client module exports only TipLayer (server components call tipProps from ./tip)", () => {
    // A function exported from a "use client" file can't be called while
    // rendering a server component — the page crashed on exactly that.
    const src = readFileSync(new URL("./StatTip.tsx", import.meta.url), "utf8");
    const exported = [...src.matchAll(/export (?:function|const) (\w+)/g)].map((m) => m[1]);
    expect(exported).toEqual(["TipLayer"]);
  });

  it("a scroll event that didn't move the page keeps the card; a real scroll closes it", () => {
    const { container } = render(
      <TipLayer>
        <span {...tipProps({ title: "steady" })}>x</span>
      </TipLayer>,
    );
    const card = container.querySelector(".stip-card")!;
    window.scrollTo(0, 0);
    fireEvent.pointerMove(container.querySelector("span")!, { clientX: 5, clientY: 5, pointerType: "mouse" });
    fireEvent.scroll(window);
    expect(card.hasAttribute("data-open")).toBe(true);
    window.scrollTo(0, 200);
    fireEvent.scroll(window);
    expect(card.hasAttribute("data-open")).toBe(false);
    window.scrollTo(0, 0);
  });

  it("nested layers open only the innermost card", () => {
    const { container } = render(
      <TipLayer>
        <TipLayer>
          <span {...tipProps({ title: "inner" })}>x</span>
        </TipLayer>
      </TipLayer>,
    );
    fireEvent.pointerMove(container.querySelector("span")!, { clientX: 5, clientY: 5, pointerType: "mouse" });
    const open = [...container.querySelectorAll(".stip-card[data-open]")];
    expect(open.length).toBe(1);
    expect(open[0].textContent).toContain("inner");
  });

  it("shows the nearest tip on hover and focus, hides on leave and Escape", () => {
    const { container } = render(
      <TipLayer>
        <svg>
          <rect {...tipProps({ title: "Bash", rows: [["calls", "11,778"]], note: "Your favourite." })} tabIndex={0}>
            <tspan>x</tspan>
          </rect>
        </svg>
      </TipLayer>,
    );
    const card = container.querySelector(".stip-card")!;
    const rect = container.querySelector("rect")!;
    expect(card.hasAttribute("data-open")).toBe(false);
    fireEvent.pointerMove(rect.firstChild as Element, { clientX: 10, clientY: 10, pointerType: "mouse" });
    expect(card.hasAttribute("data-open")).toBe(true);
    expect(card.textContent).toContain("Bash");
    expect(card.textContent).toContain("11,778");
    expect(card.textContent).toContain("Your favourite.");
    fireEvent.pointerLeave(container.querySelector(".stip-layer")!, { pointerType: "mouse" });
    expect(card.hasAttribute("data-open")).toBe(false);
    fireEvent.focus(rect);
    expect(card.hasAttribute("data-open")).toBe(true);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(card.hasAttribute("data-open")).toBe(false);
  });
});
