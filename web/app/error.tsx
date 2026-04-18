"use client";

// Global error boundary for the App Router. Without this, any error
// thrown during server-rendering a day page (e.g. `listBlocksForDay`
// fails because the DB is locked or schema-mismatched) produces a
// blank white page with no recovery path. Here we render a plain
// readable message + the actual error string + a retry button.

import { useEffect } from "react";
import { AlertCircle, RefreshCw } from "lucide-react";

interface Props {
  error: Error & { digest?: string };
  reset: () => void;
}

export default function Error({ error, reset }: Props) {
  useEffect(() => {
    // Surface to the browser console so the user can file a bug report
    // with the full stack if needed.
    // eslint-disable-next-line no-console
    console.error("[worklog] page error:", error);
  }, [error]);

  return (
    <div className="empty-state" style={{ textAlign: "left" }}>
      <AlertCircle
        size={28}
        strokeWidth={1.5}
        style={{ color: "var(--terracotta)", marginBottom: 10 }}
      />
      <h2>Couldn&rsquo;t load this day.</h2>
      <p style={{ marginBottom: 12 }}>
        Something went wrong reading the worklog database or reaching
        the Rust daemon. The most common causes:
      </p>
      <ul style={{ marginLeft: 20, marginBottom: 16, color: "var(--fg-muted)" }}>
        <li>The daemon isn&rsquo;t running — try <code>worklog daemon</code>.</li>
        <li>The SQLite DB file is locked by another process.</li>
        <li>The DB schema drifted from what this build expects — run <code>worklog db migrate</code>.</li>
      </ul>
      <pre
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: 12,
          padding: 12,
          background: "var(--bg-sunk)",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius-sm)",
          overflowX: "auto",
          whiteSpace: "pre-wrap",
        }}
      >
        {error.message}
        {error.digest ? `\n\ndigest: ${error.digest}` : ""}
      </pre>
      <button
        type="button"
        className="action-btn"
        onClick={() => reset()}
        style={{ marginTop: 16 }}
      >
        <RefreshCw />
        Retry
      </button>
    </div>
  );
}
