import { chipOf } from "./TaskDayGroup";
import type { BlocksLoad } from "./useWorkLog";

export interface TempoState {
  text: string;
  tone: "ok" | "none" | "changed";
  /** There is a day to go and fix; the text then comes with a Show unsent day action. */
  review: boolean;
}

const days = (n: number) => `${n} ${n === 1 ? "day" : "days"}`;

/** Which days still need Tempo, in plain words; null until the work log has loaded with at least one day. */
export function tempoState(load: BlocksLoad): TempoState | null {
  if (load.s !== "ok" || load.data.days.length === 0) return null;
  const states = load.data.days.map((d) => chipOf(d.blocks));
  const none = states.filter((s) => s === "Not sent").length;
  const changed = states.filter((s) => s === "Changed since sent").length;
  if (none === 0 && changed === 0) return { text: "All sent", tone: "ok", review: false };
  const text = [none > 0 && `${days(none)} not sent`, changed > 0 && `${days(changed)} changed since sent`].filter(Boolean).join(", ");
  return { text, tone: none > 0 ? "none" : "changed", review: true };
}
