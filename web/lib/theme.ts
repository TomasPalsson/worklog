// Theme resolution + persistence.
//
// Three states the UI cares about:
//   - "light" / "dark": user clicked the toggle to pick this explicitly.
//     Wins over the system preference.
//   - "system" (null cookie): follow prefers-color-scheme. We express
//     this by removing the cookie + the data-theme attribute, letting
//     the @media fallback in globals.css do the work.
//
// The cookie is surfaced to the server render (layout.tsx reads it via
// next/headers::cookies()) so SSR emits <html data-theme="…"> directly —
// no flash of wrong theme on first paint. A tiny inline script in the
// document head handles the system-preference branch (no cookie) before
// first paint.

export type ThemePreference = "light" | "dark" | "system";

export const COOKIE_NAME = "theme";
const COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 365; // 1 year

/**
 * Pure parser — extracts the theme value from a `document.cookie`-style
 * string. Exposed for tests so we can assert on edge cases without a DOM.
 * Only "light" or "dark" are honoured; anything else (empty, unknown,
 * tampered) resolves to null so the system default wins.
 */
export function parseThemeCookie(
  cookieString: string,
): "light" | "dark" | null {
  const match = cookieString
    .split("; ")
    .find((row) => row.startsWith(`${COOKIE_NAME}=`));
  if (!match) return null;
  const value = match.split("=")[1];
  if (value === "light" || value === "dark") return value;
  return null;
}

/**
 * Pure serialiser — the Set-Cookie fragment we'd write for a given
 * theme choice. `null` → eviction string (max-age=0).
 */
export function formatThemeCookie(value: "light" | "dark" | null): string {
  if (value === null) {
    return `${COOKIE_NAME}=; path=/; max-age=0; SameSite=Lax`;
  }
  return `${COOKIE_NAME}=${value}; path=/; max-age=${COOKIE_MAX_AGE_SECONDS}; SameSite=Lax`;
}

export function readThemeCookie(): "light" | "dark" | null {
  if (typeof document === "undefined") return null;
  return parseThemeCookie(document.cookie);
}

export function writeThemeCookie(value: "light" | "dark" | null): void {
  if (typeof document === "undefined") return;
  document.cookie = formatThemeCookie(value);
}

/**
 * Apply a theme choice to the live DOM. `null` = defer to system: we
 * remove the data-theme attr so the @media fallback in globals.css wins.
 */
export function applyThemeAttr(value: "light" | "dark" | null): void {
  if (typeof document === "undefined") return;
  const html = document.documentElement;
  if (value === null) {
    html.removeAttribute("data-theme");
  } else {
    html.dataset.theme = value;
  }
  // Keep the theme-color meta in sync with the now-current bg so the OS
  // title bar / Safari chrome match. We resolve the actual theme (which
  // may be system-decided if value===null) by reading the attribute or
  // falling back to the media query.
  const meta = document.querySelector<HTMLMetaElement>(
    'meta[name="theme-color"]',
  );
  if (!meta) return;
  const resolved =
    value ??
    (window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light");
  meta.content = resolved === "dark" ? "#1f1d1b" : "#fafaf7";
}
