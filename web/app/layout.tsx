import type { Metadata, Viewport } from "next";
import { cookies } from "next/headers";
import { GeistSans } from "geist/font/sans";
import { GeistMono } from "geist/font/mono";
import { ToastHost } from "@/components/ToastHost";
import "./globals.css";

export const metadata: Metadata = {
  title: "worklog",
  description: "Personal work time review",
};

export const viewport: Viewport = {
  // Two entries so the OS chrome matches both themes when no cookie is
  // set. When the cookie IS set, ThemeToggle updates the meta dynamically
  // so this falls back cleanly.
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#fafaf7" },
    { media: "(prefers-color-scheme: dark)", color: "#1f1d1b" },
  ],
  width: "device-width",
  initialScale: 1,
};

/**
 * Inline script injected into <head> before any CSS parses. If the user
 * has a theme cookie we already emit data-theme on <html> from the server,
 * so this is a no-op. If they DON'T have a cookie (system-default path),
 * this still runs and does nothing — the @media (prefers-color-scheme)
 * fallback in globals.css picks up the system setting without help.
 *
 * The cookie-wins-over-system logic is fully server-side; this script
 * only exists so future additions (e.g. hydration-time overrides) have
 * a preserved injection point with a nonce-compatible shape.
 */
const noFlashScript = `
  try {
    var m = document.cookie.match(/(?:^|; )theme=(light|dark)/);
    if (m) { document.documentElement.setAttribute('data-theme', m[1]); }
  } catch (_) {}
`;

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const cookieStore = await cookies();
  const stored = cookieStore.get("theme")?.value;
  const initialTheme =
    stored === "light" || stored === "dark" ? stored : undefined;

  return (
    <html
      lang="en"
      className={`${GeistSans.variable} ${GeistMono.variable}`}
      {...(initialTheme ? { "data-theme": initialTheme } : {})}
    >
      <head>
        <script dangerouslySetInnerHTML={{ __html: noFlashScript }} />
      </head>
      <body>
        <main className="page">{children}</main>
        <ToastHost />
      </body>
    </html>
  );
}
