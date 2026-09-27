"use client";

// The day page's per-day "done elsewhere" list (spec 006, FR-05/FR-06):
// org commits/PRs whose sha is in no local clone, moved into a chosen
// block by hand. Mirrors UnsortedList's look.

import type { Block } from "@/lib/types";
import type { ElsewhereItem } from "@/lib/daemonElsewhere";

interface Props {
  day: string;
  items: ElsewhereItem[];
  blocks: Block[];
}

export function ElsewhereList(_props: Props) {
  throw new Error("T006 GREEN");
}
