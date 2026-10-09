// UTC-safe day helpers shared by the time charts. Days are YYYY-MM-DD.
export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
export const WEEKDAYS_LONG = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
export const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export const parseDay = (day: string) => new Date(day + "T00:00:00Z");
/** 0 = Monday .. 6 = Sunday. */
export const dowMon = (day: string) => (parseDay(day).getUTCDay() + 6) % 7;
export const shortLabel = (day: string) => {
  const d = parseDay(day);
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
};
export const dayLabel = (day: string) => `${WEEKDAYS[dowMon(day)]} ${shortLabel(day)}`;

/** "HH:MM" -> minutes since midnight; null when missing or unparsable. */
export function hmToMin(s: string | null): number | null {
  if (!s) return null;
  const m = /^(\d{1,2}):(\d{2})$/.exec(s);
  return m ? Number(m[1]) * 60 + Number(m[2]) : null;
}
export const minToHm = (min: number) => {
  const r = Math.round(min);
  return `${String(Math.floor(r / 60)).padStart(2, "0")}:${String(r % 60).padStart(2, "0")}`;
};
