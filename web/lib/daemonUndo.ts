// Daemon client fn for undoing the last block change (spec 018).

import { call } from "./daemon";
import type { UndoOutcome } from "./daily_helpers_contract";

export async function undoLast(): Promise<UndoOutcome> {
  return call("POST", "/undo");
}
