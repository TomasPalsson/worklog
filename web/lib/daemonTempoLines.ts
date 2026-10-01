// Daemon client fns for Tempo ticket lines (spec 011) — shares
// lib/daemon.ts's `call` transport.

import { call } from "./daemon";
import type { TempoLine, TempoLineKey } from "./tempo_line_contract";

export async function tempoLines(day: string): Promise<TempoLine[]> {
  return call("GET", `/tempo/lines/${encodeURIComponent(day)}`);
}

/** A blank `text` clears the stored text back to generated-or-fallback. */
export async function setTempoLineText(key: TempoLineKey, text: string): Promise<TempoLine> {
  return call("POST", "/tempo/lines/text", { ...key, text });
}

/** `null` clears the override; the daemon rejects non-positive or non-half-hour values. */
export async function setTempoLineHours(
  key: TempoLineKey,
  seconds: number | null,
): Promise<TempoLine> {
  return call("POST", "/tempo/lines/hours", { ...key, seconds });
}

export async function regenerateTempoLine(key: TempoLineKey): Promise<TempoLine> {
  return call("POST", "/tempo/lines/regenerate", key);
}
