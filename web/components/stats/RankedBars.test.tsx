import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { formatValue, RankedBars } from "./RankedBars";

afterEach(cleanup);

describe("formatValue", () => {
  it("formats per unit and survives junk", () => {
    expect(formatValue("count", 4213)).toBe("4,213");
    expect(formatValue("minutes", 192)).toBe("3h 12m");
    expect(formatValue("seconds", 5400)).toBe("1h 30m");
    expect(formatValue("count", NaN)).toBe("0");
    expect(formatValue("count", 1234567890)).toBe("1,234,567,890");
  });
});

describe("RankedBars", () => {
  it("scales to the top row, highlights it, writes caption and aria", () => {
    const { container, getByRole, getByText } = render(
      <RankedBars title="Tools" unit="count" rows={[{ label: "Bash", value: 4213 }, { label: "Read", value: 2106.5 }]} />,
    );
    expect(getByText("Bash leads with 4,213")).toBeTruthy();
    const fills = container.querySelectorAll<HTMLElement>(".sr-fill");
    expect(fills[0].style.width).toBe("100%");
    expect(fills[1].style.width).toBe("50%");
    expect(container.querySelectorAll("[data-top]").length).toBe(1);
    expect(getByRole("img").getAttribute("aria-label")).toBe("Tools: Bash 4,213, Read 2,107");
  });

  it("empty and all-zero rows show the calm state, no NaN", () => {
    for (const rows of [[], [{ label: "a", value: 0 }]]) {
      const { container, getByText } = render(<RankedBars title="T" unit="minutes" rows={rows} />);
      expect(getByText("nothing here yet")).toBeTruthy();
      expect(container.innerHTML).not.toContain("NaN");
      cleanup();
    }
  });
});
