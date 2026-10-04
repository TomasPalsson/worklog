import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";

let path = "/";
const navPath = "next/dist/client/components/navigation";
const real = { ...(await import(navPath)) };
mock.module(navPath, () => ({ ...real, usePathname: () => path }));
const { AppNav } = await import("./AppNav");
afterEach(cleanup);

describe("AppNav", () => {
  it("renders the items in order", () => {
    path = "/";
    const { container } = render(<AppNav />);
    const items = [...container.querySelectorAll(".app-nav-links > *")].map((e) => e.textContent);
    expect(items).toEqual(["Day", "Week", "Tasks", "Logged", "Settings", "Billing"]);
  });

  it("links Billing to /billing and Settings is a button", () => {
    render(<AppNav />);
    expect(screen.getByRole("link", { name: "Billing" }).getAttribute("href")).toBe("/billing");
    expect(screen.getByRole("button", { name: "Settings" }).getAttribute("aria-haspopup")).toBe("dialog");
  });

  const cases: [string, string][] = [
    ["/", "Day"],
    ["/2026-10-01", "Day"],
    ["/week/2026-09-28", "Week"],
    ["/tasks", "Tasks"],
    ["/logged/month/2026-10", "Logged"],
    ["/billing", "Billing"],
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
