/** End of the day's latest non-ignored block, or null when there is none. */
export function lastBlockEnd(blocks: { ended_at: string; ignored_at?: string | null }[]): string | null {
  let latest: string | null = null;
  for (const b of blocks) {
    if (b.ignored_at) continue;
    if (latest === null || Date.parse(b.ended_at) > Date.parse(latest)) latest = b.ended_at;
  }
  return latest;
}
