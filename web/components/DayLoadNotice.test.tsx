import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { DayLoadNotice, sectionLoader } from "./DayLoadNotice";

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

  it("uses the styled error class", () => {
    // catches: an undefined class that renders as plain body text
    render(<DayLoadNotice failed={["Recap"]} />);
    expect(screen.getByRole("alert").className).toBe("review-sec-error");
  });
});

describe("sectionLoader", () => {
  it("returns the fallback and records the name on rejection", async () => {
    // catches: rethrowing, or dropping the name
    const failed: string[] = [];
    const load = sectionLoader(failed);
    expect(await load("Recap", Promise.reject(new Error("x")), null)).toBeNull();
    expect(failed).toEqual(["Recap"]);
  });

  it("passes resolved values through without recording", async () => {
    // catches: pushing a name on success, or replacing the value with the fallback
    const failed: string[] = [];
    const load = sectionLoader(failed);
    expect(await load("Tempo lines", Promise.resolve([1]), [] as number[])).toEqual([1]);
    expect(failed).toEqual([]);
  });

  it("one rejection leaves siblings intact under Promise.all", async () => {
    // catches: a failure rejecting the whole page load
    const failed: string[] = [];
    const load = sectionLoader(failed);
    const [a, b, c] = await Promise.all([
      load("Elsewhere items", Promise.reject(new Error("x")), [] as string[]),
      load("Tempo lines", Promise.resolve("ok"), ""),
      load("Recap", Promise.resolve(7), 0),
    ]);
    expect([a, b, c]).toEqual([[], "ok", 7]);
    expect(failed).toEqual(["Elsewhere items"]);
  });
});
