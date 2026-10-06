"use client";

import { Fragment } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { BillingIcon, DayIcon, LoggedIcon, LogoMark, MirresIcon, SettingsIcon, TasksIcon, WeekIcon } from "./icons";
import { ThemeToggle } from "./ThemeToggle";

const DAY = /^\/(\d{4}-\d{2}-\d{2})$/;

const LINKS = [
  ["day", "Day", "/", DayIcon],
  ["week", "Week", "/week", WeekIcon],
  ["tasks", "Tasks", "/tasks", TasksIcon],
  ["logged", "Logged", "/logged", LoggedIcon],
  ["billing", "Billing", "/billing", BillingIcon],
  ["mirres", "Mirres", "/mirres", MirresIcon],
  ["settings", "Settings", "/settings", SettingsIcon],
] as const;

function sectionOf(path: string): string {
  if (path === "/" || DAY.test(path)) return "day";
  return ["week", "tasks", "logged", "billing", "mirres", "settings"].find((s) => path === `/${s}` || path.startsWith(`/${s}/`)) ?? "";
}

export function AppNav() {
  const path = usePathname() ?? "/";
  const current = sectionOf(path);
  // From a day page, Settings links back to (and refreshes) that day.
  const day = DAY.exec(path)?.[1];

  return (
    <nav className="app-rail" aria-label="Main">
      <Link href="/" className="app-rail-logo" aria-label="worklog home">
        <LogoMark size={28} />
      </Link>
      {LINKS.map(([key, label, href, Icon]) => (
        <Fragment key={key}>
          {key === "billing" && <span className="app-rail-spacer" aria-hidden="true" />}
          <Link
            href={key === "settings" && day ? `${href}?from=${day}` : href}
            className="app-rail-item"
            aria-current={key === current ? "page" : undefined}
          >
            <Icon />
            <span className="app-rail-label">{label}</span>
          </Link>
        </Fragment>
      ))}
      <ThemeToggle className="app-rail-item" />
    </nav>
  );
}
