"use client";

import { Fragment } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { todayISO } from "@/lib/format";
import { BillingIcon, DayIcon, LoggedIcon, LogoMark, MirresIcon, TasksIcon, WeekIcon } from "./icons";
import { SettingsPanel } from "./SettingsPanel";
import { ThemeToggle } from "./ThemeToggle";

const DAY = /^\/(\d{4}-\d{2}-\d{2})$/;

const LINKS = [
  ["day", "Day", "/", DayIcon],
  ["week", "Week", "/week", WeekIcon],
  ["tasks", "Tasks", "/tasks", TasksIcon],
  ["logged", "Logged", "/logged", LoggedIcon],
  ["billing", "Billing", "/billing", BillingIcon],
  ["mirres", "Mirres", "/mirres", MirresIcon],
  ["settings", "Settings", "", null],
] as const;

function sectionOf(path: string): string {
  if (path === "/" || DAY.test(path)) return "day";
  return ["week", "tasks", "logged", "billing", "mirres"].find((s) => path === `/${s}` || path.startsWith(`/${s}/`)) ?? "";
}

export function AppNav() {
  const path = usePathname() ?? "/";
  const current = sectionOf(path);

  return (
    <nav className="app-rail" aria-label="Main">
      <Link href="/" className="app-rail-logo" aria-label="worklog home">
        <LogoMark size={28} />
      </Link>
      {LINKS.map(([key, label, href, Icon]) =>
        Icon ? (
          <Fragment key={key}>
            {key === "billing" && <span className="app-rail-spacer" aria-hidden="true" />}
            <Link
              href={href}
              className="app-rail-item"
              aria-current={key === current ? "page" : undefined}
            >
              <Icon />
              <span className="app-rail-label">{label}</span>
            </Link>
          </Fragment>
        ) : (
          <SettingsPanel key={key} day={DAY.exec(path)?.[1] ?? todayISO()} variant="menu" />
        ),
      )}
      <ThemeToggle className="app-rail-item" />
    </nav>
  );
}
