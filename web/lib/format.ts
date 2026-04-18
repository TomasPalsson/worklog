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
