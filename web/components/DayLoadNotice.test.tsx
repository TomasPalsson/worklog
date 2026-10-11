import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { DayLoadNotice } from "./DayLoadNotice";

afterEach(cleanup);

describe("DayLoadNotice", () => {
  it("renders nothing when no section failed", () => {
    // catches: always rendering a banner
    render(<DayLoadNotice failed={[]} />);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("names the single failed section", () => {
    // catches: a generic message that swallows which call failed
    render(<DayLoadNotice failed={["Tempo lines"]} />);
    expect(screen.getByRole("alert").textContent).toContain("Tempo lines");
  });

  it("names every failed section in one notice", () => {
    // catches: one alert per section, or only the first name shown
    render(<DayLoadNotice failed={["Elsewhere", "Recap", "Verdict status"]} />);
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    for (const n of ["Elsewhere", "Recap", "Verdict status"]) {
      expect(alerts[0].textContent).toContain(n);
    }
  });

  it("does not name sections that loaded", () => {
    // catches: listing every section regardless of failure
    render(<DayLoadNotice failed={["Recap"]} />);
    expect(screen.getByRole("alert").textContent).not.toContain("Tempo lines");
  });
});
