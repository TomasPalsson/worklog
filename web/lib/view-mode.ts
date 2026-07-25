// Day-view mode + persistence.
//
// Two ways to look at the same day:
//   - "tickets": the original review view — blocks grouped by Jira ticket,
//     with the ticket picker and Tempo sync state on every card. What you
//     want while assigning tickets.
//   - "billing": blocks grouped the way the export bills them (customer,
//     verkefni, ticket-or-folder), with ticket and sync UI hidden. What you
//     want while filling in the invoicing form.
//
// Persisted in a cookie rather than localStorage so the server render picks
// it up (the day page reads it via next/headers::cookies()) and the right
// view is in the first paint — no flash of the wrong grouping. Mirrors
// `lib/theme.ts` deliberately; same shape, same reasoning.

export type ViewMode = "tickets" | "billing";

export const COOKIE_NAME = "day-view";
const COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 365; // 1 year

/** The view used when no cookie is set. */
export const DEFAULT_VIEW: ViewMode = "tickets";

/**
 * Pure parser over a `document.cookie`-style string. Anything unknown
 * (empty, tampered, an old value) resolves to the default rather than
 * throwing — a bad cookie should never break the day page.
 */
export function parseViewCookie(cookieString: string): ViewMode {
  const match = cookieString
    .split("; ")
    .find((row) => row.startsWith(`${COOKIE_NAME}=`));
  if (!match) return DEFAULT_VIEW;
  return normaliseView(match.split("=")[1]);
}

/** Coerce an arbitrary string (cookie value, query param) to a ViewMode. */
export function normaliseView(value: string | undefined | null): ViewMode {
  return value === "billing" || value === "tickets" ? value : DEFAULT_VIEW;
}

/** The Set-Cookie fragment for a given choice. */
export function formatViewCookie(value: ViewMode): string {
  return `${COOKIE_NAME}=${value}; path=/; max-age=${COOKIE_MAX_AGE_SECONDS}; SameSite=Lax`;
}

export function readViewCookie(): ViewMode {
  if (typeof document === "undefined") return DEFAULT_VIEW;
  return parseViewCookie(document.cookie);
}

export function writeViewCookie(value: ViewMode): void {
  if (typeof document === "undefined") return;
  document.cookie = formatViewCookie(value);
}
