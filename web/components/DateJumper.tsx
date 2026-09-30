"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Calendar } from "lucide-react";
import { mondayOf } from "@/lib/format";

interface Props {
  /** The currently focused day, used to seed the date input. Defaults
   * to the week's Monday so the picker opens on something the user
   * recognises. */
  focusedDay: string;
}

/**
 * Native `<input type="date">` driven jump-to-week picker. Browsers
 * render this as a real calendar popup on every platform we care about
 * (Chromium, Safari, Firefox); no JS calendar dep needed.
 *
 * Submitting any day in a week navigates to that week's Monday.
 */
export function WeekJumper({ focusedDay }: Props) {
  const router = useRouter();
  const [value, setValue] = useState(focusedDay);

  function go(next: string) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(next)) return;
    router.push(`/week/${mondayOf(next)}`);
  }

  return (
    <form
      className="week-jumper"
      onSubmit={(e) => {
        e.preventDefault();
        go(value);
      }}
    >
      <label className="week-jumper-label">
        <Calendar size={14} strokeWidth={1.75} aria-hidden />
        <span className="visually-hidden">jump to date</span>
        <input
          type="date"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          // Submit immediately on picker selection so users don't need
          // the explicit button. Form fallback covers keyboard users
          // who type the date manually and press Enter.
          onBlur={() => value !== focusedDay && go(value)}
          aria-label="jump to date"
          className="week-jumper-input"
        />
      </label>
      <button type="submit" className="week-jumper-go">
        Jump
      </button>
    </form>
  );
}
