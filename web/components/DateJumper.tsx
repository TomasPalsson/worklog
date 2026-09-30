"use client";

import { useRef } from "react";
import { useRouter } from "next/navigation";
import { Calendar } from "lucide-react";
import { mondayOf } from "@/lib/format";

interface Props {
  /** The day the picker opens on. */
  focusedDay: string;
  /** Which page a picked date opens: that day, or the week holding it. */
  view: "day" | "week";
}

/**
 * Arrow-sized jump-to-date button: a native date input stretched invisibly
 * over the icon, so browsers show their own calendar. A wider control made
 * the nav overflow and drift with the heading. Picking a date navigates.
 */
export function DateJumper({ focusedDay, view }: Props) {
  const router = useRouter();
  const input = useRef<HTMLInputElement>(null);

  function go(next: string) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(next) || next === focusedDay) return;
    // Typing a year by keyboard passes through 0002, 0020, 0202 first.
    if (Number(next.slice(0, 4)) < 1000) return;
    router.push(view === "week" ? `/week/${mondayOf(next)}` : `/${next}`);
  }

  return (
    <label className="day-nav-btn date-jumper" data-tip="Jump to date">
      <Calendar size={16} strokeWidth={1.75} aria-hidden />
      <input
        // Re-seed when the arrows move to another day: the component is
        // reused across client navigations.
        key={focusedDay}
        ref={input}
        type="date"
        defaultValue={focusedDay}
        onChange={(e) => go(e.target.value)}
        // Chromium only opens the calendar from its own indicator.
        onClick={() => {
          try {
            input.current?.showPicker?.();
          } catch {
            // Unsupported or not user-activated; the native click still works.
          }
        }}
        aria-label="jump to date"
        className="date-jumper-input"
      />
    </label>
  );
}
