"use client";

// Section index + card headings for /settings: one icon per topic, shown
// in both places so the index and the card you land on match. The index
// marks the card currently in view.

import { useEffect, useState } from "react";
import { CalendarX2, FolderTree, Globe, Plug, Route, Scale, type LucideIcon } from "lucide-react";

export const SECTIONS: [id: string, label: string, Icon: LucideIcon][] = [
  ["verdict", "Verdict", Scale],
  ["folders", "Work or personal", FolderTree],
  ["time", "Time zone", Globe],
  ["cleanup", "Old data cleanup", CalendarX2],
  ["sorting", "Browser & Slack", Route],
  ["connections", "Connections", Plug],
];

const ICON = Object.fromEntries(SECTIONS.map(([id, , Icon]) => [id, Icon]));

/** A card's h2 with its section icon. `id` must be a SECTIONS id. */
export function CardHead({ id, title }: { id: string; title: string }) {
  const Icon = ICON[id];
  return (
    <h2 id={`${id}-title`} className="set-card-head">
      {Icon && (
        <span className="set-card-icon" aria-hidden="true">
          <Icon size={18} strokeWidth={1.75} />
        </span>
      )}
      {title}
    </h2>
  );
}

/** Which section is in view: the last one whose top has passed 30% of the viewport. */
function useActiveSection(): string {
  const [active, setActive] = useState<string>(SECTIONS[0][0]);
  useEffect(() => {
    const onScroll = () => {
      const line = window.innerHeight * 0.3;
      let current = SECTIONS[0][0];
      for (const [id] of SECTIONS) {
        const el = document.getElementById(id);
        if (el && el.getBoundingClientRect().top <= line) current = id;
      }
      // At the very bottom the last card may never reach the line.
      if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2) {
        current = SECTIONS[SECTIONS.length - 1][0];
      }
      setActive(current);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);
  return active;
}

export function SettingsIndex({ connections }: { connections: { done: number; total: number } }) {
  const active = useActiveSection();
  // Phone layout: keep the current chip visible in the sideways-scrolling row.
  useEffect(() => {
    document
      .querySelector(`.set-index a[href="#${active}"]`)
      ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [active]);
  return (
    <nav className="set-index" aria-label="Settings sections">
      <ol>
        {SECTIONS.map(([id, label, Icon]) => (
          <li key={id}>
            <a href={`#${id}`} aria-current={id === active ? "true" : undefined}>
              <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
              <span className="set-index-label">{label}</span>
              {id === "connections" && connections.total > 0 && (
                <span className="set-index-count">
                  {connections.done}/{connections.total}
                </span>
              )}
            </a>
          </li>
        ))}
      </ol>
    </nav>
  );
}
