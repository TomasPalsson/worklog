"use client";

import type { PullReport, WeekCloseout as Closeout } from "@/lib/types";
import type { WeekSyncDeps } from "@/lib/weekSync";

type Result<T> = { ok: true; data: T } | { ok: false; error: string };

export interface CloseoutActions extends WeekSyncDeps {
  loadCloseout: (monday: string) => Promise<Result<Closeout>>;
}

export function WeekCloseout(_props: {
  closeout: Closeout;
  actions?: CloseoutActions;
}): React.ReactElement {
  throw new Error("not implemented");
}

export type { PullReport };
