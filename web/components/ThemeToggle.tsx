"use client";

import { useEffect, useState } from "react";
import { MoonIcon, SunIcon, SystemThemeIcon } from "./icons";
import {
  applyThemeAttr,
  readThemeCookie,
  writeThemeCookie,
  type ThemePreference,
} from "@/lib/theme";

// Ordered cycle: "system" → "light" → "dark" → back to "system".
// Three states so the user never loses access to "follow OS" after
// overriding once.
const ORDER: ThemePreference[] = ["system", "light", "dark"];

function nextState(current: ThemePreference): ThemePreference {
  const i = ORDER.indexOf(current);
  return ORDER[(i + 1) % ORDER.length];
}

function iconFor(state: ThemePreference) {
  if (state === "light") return <SunIcon />;
  if (state === "dark") return <MoonIcon />;
  return <SystemThemeIcon />;
}

function shortFor(state: ThemePreference): string {
  return state === "system" ? "System" : state === "light" ? "Light" : "Dark";
}

function labelFor(state: ThemePreference): string {
  if (state === "light") return "Theme: light (click to switch to dark)";
  if (state === "dark") return "Theme: dark (click to follow system)";
  return "Theme: follow system (click to switch to light)";
}

/**
 * Three-state theme toggle — system → light → dark → system. Initial
 * state comes from the cookie so SSR + client agree on first paint.
 * The inline no-flash script in layout.tsx handles the pre-React frame.
 */
export function ThemeToggle({ className = "theme-toggle" }: { className?: string }) {
  // Always start "system" on the server so hydration doesn't mismatch.
  // A useEffect below reconciles with the cookie as soon as we hit the
  // client.
  const [state, setState] = useState<ThemePreference>("system");

  useEffect(() => {
    const stored = readThemeCookie();
    if (stored) setState(stored);
  }, []);

  function toggle() {
    const next = nextState(state);
    setState(next);
    if (next === "system") {
      writeThemeCookie(null);
      applyThemeAttr(null);
    } else {
      writeThemeCookie(next);
      applyThemeAttr(next);
    }
  }

  return (
    <button
      type="button"
      className={className}
      onClick={toggle}
      aria-label={labelFor(state)}
      data-tip={`Theme: ${shortFor(state).toLowerCase()}`}
    >
      {iconFor(state)}
      {className !== "theme-toggle" && (
        <span className="app-rail-label app-rail-label--phone" aria-hidden="true">
          {shortFor(state)}
        </span>
      )}
    </button>
  );
}
