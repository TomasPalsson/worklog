// Display formatters — kept dependency-free (no Intl heavy config).

export function formatClock(iso: string): string {
  // ISO timestamps in our DB are UTC; we render in local time because
  // users review their own day.
  const d = new Date(iso);
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  return `${h}:${m}`;
}

export function formatRange(start: string, end: string): string {
  return `${formatClock(start)}–${formatClock(end)}`;
}

export function formatDuration(seconds: number): string {
  const totalMin = Math.round(seconds / 60);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

export function formatTotalHours(seconds: number): string {
  const hours = seconds / 3600;
  // 2.5h not 2h 30m for the header total — easier to scan against 8h targets
  return `${hours.toFixed(1)}h`;
}

export function todayISO(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function shiftDay(day: string, delta: number): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d));
  dt.setUTCDate(dt.getUTCDate() + delta);
  return dt.toISOString().slice(0, 10);
}

/**
 * Snap an ISO day to the Monday of its week. ISO week starts on Monday,
 * matching the Rust side.
 */
export function mondayOf(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d));
  // getUTCDay: Sun=0, Mon=1, … Sat=6. ISO offset is (day+6)%7.
  const offset = (dt.getUTCDay() + 6) % 7;
  dt.setUTCDate(dt.getUTCDate() - offset);
  return dt.toISOString().slice(0, 10);
}

export function shiftWeek(monday: string, deltaWeeks: number): string {
  return shiftDay(monday, deltaWeeks * 7);
}

/** The 7 ISO days starting at `monday`, in order Mon..Sun. */
export function weekDays(monday: string): string[] {
  return Array.from({ length: 7 }, (_, i) => shiftDay(monday, i));
}

/**
 * Human label for the week. "May 11–17, 2026" when both ends share a
 * month, "Dec 30 – Jan 5, 2026" when only the year is shared, and the
 * full "Dec 30, 2025 – Jan 5, 2026" when the week straddles a year.
 * Year is omitted entirely when both ends fall in the current year.
 */
export function formatWeekRange(monday: string): string {
  const [sy, sm, sd] = monday.split("-").map(Number);
  const start = new Date(Date.UTC(sy, sm - 1, sd));
  const end = new Date(start);
  end.setUTCDate(end.getUTCDate() + 6);
  const thisYear = new Date().getFullYear();
  const sameMonth = start.getUTCMonth() === end.getUTCMonth();
  const sameYear = start.getUTCFullYear() === end.getUTCFullYear();
  const showYear = !sameYear || start.getUTCFullYear() !== thisYear;
  const fmt = (d: Date, opts: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat("en-US", { timeZone: "UTC", ...opts }).format(d);

  if (sameMonth) {
    const month = fmt(start, { month: "short" });
    const yearSuffix = showYear ? `, ${start.getUTCFullYear()}` : "";
    return `${month} ${start.getUTCDate()}–${end.getUTCDate()}${yearSuffix}`;
  }
  if (sameYear) {
    const startStr = fmt(start, { month: "short", day: "numeric" });
    const endStr = fmt(end, { month: "short", day: "numeric" });
    const yearSuffix = showYear ? `, ${start.getUTCFullYear()}` : "";
    return `${startStr} – ${endStr}${yearSuffix}`;
  }
  const startStr = fmt(start, { month: "short", day: "numeric", year: "numeric" });
  const endStr = fmt(end, { month: "short", day: "numeric", year: "numeric" });
  return `${startStr} – ${endStr}`;
}

/**
 * Short weekday name for a calendar column header. "Mon" / "Tue" / …
 */
export function shortWeekday(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d));
  return new Intl.DateTimeFormat("en-US", {
    weekday: "short",
    timeZone: "UTC",
  }).format(dt);
}

/** "May 11" — no year, used for compact column headers. */
export function shortMonthDay(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d));
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(dt);
}

export function formatDayHeading(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d));
  // "Friday, April 18" — no year unless it's not this year
  const thisYear = new Date().getFullYear();
  const opts: Intl.DateTimeFormatOptions = {
    weekday: "long",
    month: "long",
    day: "numeric",
    timeZone: "UTC",
    ...(y !== thisYear ? { year: "numeric" } : {}),
  };
  return new Intl.DateTimeFormat("en-US", opts).format(dt);
}
