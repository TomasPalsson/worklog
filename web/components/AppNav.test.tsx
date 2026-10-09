import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";

let path = "/";
const real = { ...(await import("next/navigation")) };
mock.module("next/navigation", () => ({ ...real, usePathname: () => path }));
const { AppNav } = await import("./AppNav");
afterEach(cleanup);

describe("AppNav", () => {
  it("renders the items in order", () => {
    path = "/";
    const { container } = render(<AppNav />);
    const items = [...container.querySelectorAll(".app-rail-item")].map((e) => e.textContent).filter(Boolean);
    expect(items).toEqual(["Day", "Week", "Tasks", "Logged", "Statistics", "Billing", "Mirres", "Settings", "System"]);
  });

  it("links Billing to /billing and Settings to /settings with the day it came from", () => {
    path = "/2026-10-01";
    render(<AppNav />);
    expect(screen.getByRole("link", { name: "Billing" }).getAttribute("href")).toBe("/billing");
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("href")).toBe("/settings?from=2026-10-01");
  });

  const cases: [string, string][] = [
    ["/", "Day"],
    ["/2026-10-01", "Day"],
    ["/week/2026-09-28", "Week"],
    ["/tasks", "Tasks"],
    ["/logged/month/2026-10", "Logged"],
    ["/stats", "Statistics"],
    ["/billing", "Billing"],
    ["/mirres", "Mirres"],
    ["/settings", "Settings"],
  ];
  for (const [p, label] of cases) {
    it(`marks only ${label} current for ${p}`, () => {
      path = p;
      const { container } = render(<AppNav />);
      const cur = [...container.querySelectorAll('[aria-current="page"]')];
      expect(cur.map((e) => e.textContent)).toEqual([label]);
    });
  }
});
