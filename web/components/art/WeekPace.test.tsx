import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { CloseoutDay } from "@/lib/types";
import { LoggedMeter } from "../LoggedMeter";
import { WeekPace } from "./WeekPace";

const H = 3600;
const day = (d: string, tempo: number, req: number | null): CloseoutDay => ({
  day: d, logged_seconds: 0, synced_seconds: 0, tempo_seconds: tempo,
  outside_seconds: 0, required_seconds: req, unticketed_seconds: 0, pending_lines: 0,
});
const week = (t: number[]) => t.map((s, i) => day(`2026-10-0${i + 5}`, s * H, 8 * H));

afterEach(cleanup);

describe("WeekPace", () => {
  it("shows behind with a hatch gap", () => {
    render(<WeekPace days={week([8, 0, 0, 0, 0])} today="2026-10-06" worked={10 * H} />);
    expect(screen.getByTestId("pace-tempo").style.width).toBe("20%");
    expect(screen.getByTestId("pace-worked").style.width).toBe("25%");
    expect(screen.getByTestId("pace-tick").style.left).toBe("40%");
    expect(screen.getByTestId("pace-gap").style.width).toBe("20%");
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("8.0 hours behind");
    expect(screen.getByText("8.0h behind")).toBeTruthy();
    expect(screen.getByText("8.0 / 40.0h")).toBeTruthy();
  });
  it("shows ahead and caps at 100%", () => {
    render(<WeekPace days={week([8, 8, 8, 8, 12])} today="2026-10-05" worked={50 * H} />);
    expect(screen.getByTestId("pace-tempo").style.width).toBe("100%");
    expect(screen.queryByTestId("pace-gap")).toBeNull();
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("hours ahead");
  });
  it("renders nothing without required hours", () => {
    const { container } = render(<WeekPace days={[day("2026-10-05", 0, null)]} today="2026-10-05" worked={0} />);
    expect(container.innerHTML).toBe("");
  });
});

describe("LoggedMeter ghost", () => {
  it("adds ghost var and aria text only when worked > logged", () => {
    render(<LoggedMeter logged={4 * H} required={8 * H} state="under" worked={6 * H} />);
    const m = screen.getByRole("meter");
    expect(m.hasAttribute("data-ghost")).toBe(true);
    expect(m.style.getPropertyValue("--ghost")).toBe("75%");
    expect(m.getAttribute("aria-label")).toBe("4h of 8h logged, 6h worked");
  });
  it("no ghost when worked <= logged or absent", () => {
    render(<LoggedMeter logged={4 * H} required={8 * H} state="under" />);
    expect(screen.getByRole("meter").hasAttribute("data-ghost")).toBe(false);
  });
});
