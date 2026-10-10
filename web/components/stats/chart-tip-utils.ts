// Small pure helpers shared by the chart tip builders.
export const nf = new Intl.NumberFormat("en-US");

/** 1 -> "1st", 12 -> "12th", 23 -> "23rd". */
export function ordinal(n: number): string {
  const r = n % 100;
  if (r >= 11 && r <= 13) return `${n}th`;
  return `${n}${["th", "st", "nd", "rd"][n % 10] ?? "th"}`;
}

/** 1-based rank of v among values, highest first (ties share the better rank). */
export const rankOf = (v: number, all: number[]) => all.filter((x) => x > v).length + 1;

export const pct = (share: number) => `${Math.round(share * 100)}%`;

/** "2.3×" style multiple, one decimal, trailing .0 dropped. */
export const times = (x: number) => `${Number(x.toFixed(1))}×`;
