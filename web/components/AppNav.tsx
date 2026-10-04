"use client";

import { useEffect, useRef } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { todayISO } from "@/lib/format";
import { SettingsPanel } from "./SettingsPanel";
import { ThemeToggle } from "./ThemeToggle";

const DAY = /^\/(\d{4}-\d{2}-\d{2})$/;

const LINKS = [
  ["day", "Day", "/"],
  ["week", "Week", "/week"],
  ["tasks", "Tasks", "/tasks"],
  ["logged", "Logged", "/logged"],
  ["settings", "Settings", ""],
  ["billing", "Billing", "/billing"],
] as const;

function sectionOf(path: string): string {
  if (path === "/" || DAY.test(path)) return "day";
  return ["week", "tasks", "logged", "billing"].find((s) => path === `/${s}` || path.startsWith(`/${s}/`)) ?? "";
}

export function AppNav() {
  const path = usePathname() ?? "/";
  const current = sectionOf(path);
  const row = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = row.current?.querySelector<HTMLElement>('[aria-current="page"]');
    el?.scrollIntoView?.({ inline: "nearest", block: "nearest" });
  }, []);

  return (
    <nav className="app-nav" aria-label="Main">
      <span className="app-nav-mark">worklog</span>
      <div className="app-nav-links" ref={row}>
        {LINKS.map(([key, label, href]) =>
          key === "settings" ? (
            <SettingsPanel key={key} day={DAY.exec(path)?.[1] ?? todayISO()} variant="menu" />
          ) : (
            <Link
              key={key}
              href={href}
              className="app-nav-link"
              aria-current={key === current ? "page" : undefined}
            >
              {label}
            </Link>
          ),
        )}
      </div>
      <ThemeToggle />
    </nav>
  );
}
