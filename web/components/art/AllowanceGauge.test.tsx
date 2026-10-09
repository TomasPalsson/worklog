import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { AllowanceGauge, gaugeFraction, gaugeTone } from "./AllowanceGauge";

afterEach(cleanup);

describe("AllowanceGauge", () => {
  it("states the numbers in words", () => {
    render(<AllowanceGauge allowance={40} used={12} />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe("12 of 40 included hours used, 28 left");
  });

  it("tones: sage below 75%, amber 75-100%, terracotta past 100%", () => {
    expect(gaugeTone(0.74)).toBe("ok");
    expect(gaugeTone(0.75)).toBe("warn");
    expect(gaugeTone(1)).toBe("warn");
    expect(gaugeTone(1.01)).toBe("over");
  });

  it("fills proportionally and has no notch under 100%", () => {
    const { container } = render(<AllowanceGauge allowance={40} used={10} />);
    expect((container.querySelector(".art-billing-fill") as HTMLElement).style.width).toBe("25%");
    expect(container.querySelector(".art-billing-notch")).toBeNull();
  });

  it("over 100% fills completely with a notch", () => {
    const { container } = render(<AllowanceGauge allowance={40} used={50} />);
    const fill = container.querySelector(".art-billing-fill") as HTMLElement;
    expect(fill.style.width).toBe("100%");
    expect(fill.getAttribute("data-tone")).toBe("over");
    expect(container.querySelector(".art-billing-notch")).not.toBeNull();
    expect(container.querySelector("[role=img]")?.getAttribute("aria-label")).toBe("50 of 40 included hours used, 10 over");
  });

  it("unknown used shows a hatched track and the label", () => {
    const { container } = render(<AllowanceGauge allowance={40} used={null} />);
    expect(container.querySelector(".art-billing-fill")).toBeNull();
    expect(container.querySelector("[data-unknown]")).not.toBeNull();
    expect(container.textContent).toContain("used unknown");
    expect(gaugeFraction(0, 5)).toBeNull();
  });
});
