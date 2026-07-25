"use client";

// Switches the day view between the ticket-centric review layout and the
// billing layout. Two-state, because there are only two jobs: assigning
// tickets, and filling in the invoicing form.
//
// The choice lives in a cookie so the *server* render already knows which
// grouping to emit — hence `router.refresh()` after writing it rather than
// local state. Re-rendering on the server is the point: the billing grouping
// comes from the daemon's export, not from anything the client can derive.

import { useRouter } from "next/navigation";
import { useTransition } from "react";
import { Receipt, Ticket } from "lucide-react";

import { writeViewCookie, type ViewMode } from "@/lib/view-mode";

interface Props {
  /** Current mode, resolved server-side from the cookie. */
  view: ViewMode;
}

export function ViewToggle({ view }: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();

  function choose(next: ViewMode) {
    if (next === view) return;
    writeViewCookie(next);
    startTransition(() => router.refresh());
  }

  return (
    <div
      className="view-toggle"
      role="group"
      aria-label="Day view"
      aria-busy={pending || undefined}
    >
      <button
        type="button"
        className={`view-toggle-btn${view === "tickets" ? " is-active" : ""}`}
        aria-pressed={view === "tickets"}
        data-tip="Ticket view — assign tickets, see sync state"
        onClick={() => choose("tickets")}
      >
        <Ticket size={14} strokeWidth={1.75} />
        Tickets
      </button>
      <button
        type="button"
        className={`view-toggle-btn${view === "billing" ? " is-active" : ""}`}
        aria-pressed={view === "billing"}
        data-tip="Billing view — grouped as the export bills it"
        onClick={() => choose("billing")}
      >
        <Receipt size={14} strokeWidth={1.75} />
        Billing
      </button>
    </div>
  );
}
