import { call } from "./daemon";
import type { RedescribeBody, RedescribeResult } from "./redescribe";
import type { Block } from "./types";

export async function redescribe(id: number, instruction: string): Promise<RedescribeResult> {
  const body: RedescribeBody = { instruction };
  return call("POST", `/blocks/${id}/redescribe`, body);
}

export async function splitBlock(id: number, firstMinutes: number): Promise<{ first: Block; second: Block }> {
  return call("POST", `/blocks/${id}/split`, { first_minutes: firstMinutes });
}
