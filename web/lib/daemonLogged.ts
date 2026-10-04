// Daemon client fns for the Logged (Tempo daily log overview) endpoints.

import { call } from "./daemon";
import type { LoggedDay, LoggedRange } from "./logged_contract";

export async function getLogged(from: string, to: string): Promise<LoggedRange> {
  return call("GET", `/logged?from=${from}&to=${to}`);
}

export async function pullLogged(from: string, to: string): Promise<LoggedRange> {
  return call("POST", "/logged/pull", { from, to });
}

export async function dismissDay(day: string, reason: string): Promise<LoggedDay> {
  return call("POST", "/logged/dismiss", { day, reason });
}

export async function undismissDay(day: string): Promise<LoggedDay> {
  return call("POST", "/logged/undismiss", { day });
}
