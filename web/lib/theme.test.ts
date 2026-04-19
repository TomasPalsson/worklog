import { describe, expect, test } from "bun:test";
import { formatThemeCookie, parseThemeCookie } from "./theme";

// These are pure functions — no DOM dependency. The DOM wrappers
// (readThemeCookie / writeThemeCookie) are thin shells; their only
// job is to pass document.cookie through these parsers. If the parsers
// are correct, the wrappers are correct.

describe("parseThemeCookie", () => {
  test("returns null for an empty cookie string", () => {
    expect(parseThemeCookie("")).toBeNull();
  });

  test("returns null when no theme cookie is present", () => {
    expect(parseThemeCookie("session=abc; flag=1")).toBeNull();
  });

  test("returns 'dark' when cookie value is dark", () => {
    expect(parseThemeCookie("theme=dark; session=abc")).toBe("dark");
  });

  test("returns 'light' when cookie value is light", () => {
    expect(parseThemeCookie("foo=1; theme=light")).toBe("light");
  });

  test("returns null for unknown cookie values", () => {
    // Defensive — a tampered or old cookie value must NOT become an
    // arbitrary data-theme attribute. Falling back to null ensures the
    // @media system-preference fallback in globals.css takes over.
    expect(parseThemeCookie("theme=purple")).toBeNull();
    expect(parseThemeCookie("theme=")).toBeNull();
  });
});

describe("formatThemeCookie", () => {
  test("serialises 'light' with a year-long max-age and path=/", () => {
    const s = formatThemeCookie("light");
    expect(s.startsWith("theme=light;")).toBe(true);
    expect(s.includes("path=/")).toBe(true);
    expect(s.includes("SameSite=Lax")).toBe(true);
    expect(s.includes("max-age=31536000")).toBe(true);
  });

  test("serialises 'dark' the same shape", () => {
    const s = formatThemeCookie("dark");
    expect(s.startsWith("theme=dark;")).toBe(true);
  });

  test("null serialises to an eviction string (max-age=0)", () => {
    // max-age=0 tells the browser to drop the cookie immediately — this
    // is how we express "follow system" after a user previously
    // overrode to light/dark.
    const s = formatThemeCookie(null);
    expect(s.includes("max-age=0")).toBe(true);
    expect(s.includes("theme=;")).toBe(true);
  });

  test("parse(format(x)) round-trips for every state", () => {
    expect(parseThemeCookie(formatThemeCookie("light"))).toBe("light");
    expect(parseThemeCookie(formatThemeCookie("dark"))).toBe("dark");
    expect(parseThemeCookie(formatThemeCookie(null))).toBeNull();
  });
});
