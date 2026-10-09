import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { ExportedStamp, stampLabel } from "./ExportedStamp";

afterEach(cleanup);

describe("ExportedStamp", () => {
  it("full stamp is an img labelled with the date, text kept as real <text>", () => {
    const { container } = render(<ExportedStamp date="9/10/2026" />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe("Exported on 9/10/2026");
    expect(container.querySelector("textPath")?.textContent).toBe("EXPORTED");
    expect(container.textContent).toContain("9/10/2026");
    expect(container.querySelector("svg")?.getAttribute("width")).toBe("96");
  });

  it("textPath and filter reference ids that exist, unique per copy", () => {
    const { container } = render(<><ExportedStamp date="a" /><ExportedStamp date="b" /></>);
    const ids = [...container.querySelectorAll("[id]")].map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
    const href = container.querySelector("textPath")?.getAttribute("href") ?? "";
    expect(container.querySelector(`[id="${href.slice(1)}"]`)).not.toBeNull();
  });

  it("mini variant reads DONE and has no date", () => {
    const { container } = render(<ExportedStamp date="" size={28} />);
    expect(container.textContent).toBe("DONE");
    expect(container.querySelector("textPath")).toBeNull();
    expect(stampLabel("", true)).toBe("Line done");
  });
});
