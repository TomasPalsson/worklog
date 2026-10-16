import { call } from "./daemon";
import type { RedescribeBody, RedescribeResult } from "./redescribe";

export async function redescribe(id: number, instruction: string): Promise<RedescribeResult> {
  const body: RedescribeBody = { instruction };
  return call("POST", `/blocks/${id}/redescribe`, body);
}
