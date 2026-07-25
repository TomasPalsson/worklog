"use server";

import { revalidatePath } from "next/cache";
import {
  assignTicket as daemonAssignTicket,
  setDuration as daemonSetDuration,
  setDescription as daemonSetDescription,
  setPersonal as daemonSetPersonal,
  deleteBlock as daemonDeleteBlock,
  runInfer as daemonRunInfer,
  runEstimate as daemonRunEstimate,
  runSync as daemonRunSync,
  refreshJira as daemonRefreshJira,
  listBlockEvents as daemonListBlockEvents,
  listBlockCommits as daemonListBlockCommits,
  searchTickets as daemonSearchTickets,
  rememberExternalTicket as daemonRememberExternalTicket,
  loadSettings as daemonLoadSettings,
  saveSettings as daemonSaveSettings,
  listProjects as daemonListProjects,
  listAccounts as daemonListAccounts,
  createTicket as daemonCreateTicket,
  exportBilling as daemonExportBilling,
  markExported as daemonMarkExported,
  loadBillingRegistry as daemonLoadBillingRegistry,
  saveBillingCustomer as daemonSaveBillingCustomer,
  deleteBillingCustomer as daemonDeleteBillingCustomer,
  saveBillingFolder as daemonSaveBillingFolder,
  deleteBillingFolder as daemonDeleteBillingFolder,
} from "@/lib/daemon";
import type {
  BillingCustomer,
  BillingFolderMap,
  BillingRegistry,
  CommitEntry,
  CreateTicketInput,
  Event,
  ExportResponse,
  JiraProject,
  JiraTicket,
  MarkExportResponse,
  SettingsSaveResponse,
  SettingsUpdate,
  SettingsView,
  TempoAccount,
} from "@/lib/types";

/**
 * Every Server Action returns one of these. `useTransition`'s `start()`
 * swallows thrown errors, so throwing from a Server Action silently
 * leaves the UI in a "success" state. We return a tagged result instead
 * and make the caller handle both branches explicitly via the `toast`.
 */
export type ActionResult<T = undefined> =
  | { ok: true; data: T }
  | { ok: false; error: string };

/**
 * Wrap a daemon call plus its `revalidatePath` side effect so any thrown
 * exception — from either the RPC or from Next.js's cache machinery —
 * is caught and surfaced through the tagged `ActionResult`.
 *
 * `revalidatePath` can throw (misconfigured Next context, invalid path,
 * unavailable cache layer); if we didn't wrap it, a successful daemon
 * write followed by a cache-invalidation error would escape the action
 * and be swallowed by `useTransition`, leaving the UI stale with no
 * toast.
 */
async function runAction<T>(
  fn: () => Promise<T>,
  revalidateOn?: string,
): Promise<ActionResult<T>> {
  try {
    const data = await fn();
    if (revalidateOn !== undefined) {
      try {
        revalidatePath(revalidateOn);
      } catch (e) {
        // Best-effort: the write succeeded, the page just won't
        // auto-refresh. Surface as a "partial" failure so the caller
        // can decide (toast as warning vs error).
        return {
          ok: false,
          error: `write succeeded but page refresh failed: ${(e as Error).message}`,
        };
      }
    }
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

// CRUD actions — `data` is always void; callers check only `ok`.

export async function assignTicket(
  blockId: number,
  key: string | null,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonAssignTicket(blockId, key), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setDuration(
  blockId: number,
  minutes: number,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonSetDuration(blockId, minutes), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setDescription(
  blockId: number,
  description: string,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonSetDescription(blockId, description), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setPersonal(
  blockId: number,
  isPersonal: boolean,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(
    () => daemonSetPersonal(blockId, isPersonal),
    `/${day}`,
  );
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function deleteBlock(
  blockId: number,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonDeleteBlock(blockId), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

// Query-style actions — `data` carries the daemon's response payload.

export async function runInfer(day: string) {
  return runAction(() => daemonRunInfer(day), `/${day}`);
}

export async function runEstimate(day: string) {
  return runAction(() => daemonRunEstimate(day), `/${day}`);
}

export async function runSync(day: string, dryRun: boolean) {
  return runAction(() => daemonRunSync(day, dryRun), `/${day}`);
}

export async function refreshJira(day: string) {
  return runAction(() => daemonRefreshJira(), `/${day}`);
}

/**
 * Fetch the events linked to a block. Lazy — called by the per-block
 * drill-down on first expand. No revalidation side effect because the
 * events don't change in response to UI actions.
 */
export async function fetchBlockEvents(
  blockId: number,
): Promise<ActionResult<Event[]>> {
  return runAction(() => daemonListBlockEvents(blockId));
}

/**
 * Fetch the commits that landed inside the block's window under its
 * dominant project path. Lazy — called by the per-block commits
 * drill-down on first expand. Returns `[]` for personal blocks and
 * blocks with no dominant cwd, so callers always get an array.
 */
export async function fetchBlockCommits(
  blockId: number,
): Promise<ActionResult<CommitEntry[]>> {
  return runAction(() => daemonListBlockCommits(blockId));
}

/**
 * Live Jira search for tickets the user is not assigned to. The picker
 * debounces typed input and calls this. Results stay ephemeral — they
 * are only persisted when the user actually picks one (via
 * `assignExternalTicket`), so the estimator never sees them.
 */
export async function searchJiraTickets(
  q: string,
): Promise<ActionResult<JiraTicket[]>> {
  return runAction(() => daemonSearchTickets(q));
}

/**
 * Pick a ticket that came from the live search. Records it in the local
 * cache with `external = 1` (so the picker can render its summary on
 * subsequent visits) and then assigns it to the block in one server
 * action. Two daemon calls; one round trip from the client.
 */
export async function assignExternalTicket(
  blockId: number,
  ticket: JiraTicket,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(async () => {
    await daemonRememberExternalTicket(ticket);
    await daemonAssignTicket(blockId, ticket.key);
  }, `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

// ───────────────────────── create ticket ─────────────────────────

/** Jira projects for the create-ticket project picker. Read-only. */
export async function fetchProjects(): Promise<ActionResult<JiraProject[]>> {
  return runAction(() => daemonListProjects());
}

/** Tempo accounts (the customer mapping) for the create-ticket account
 * picker. Read-only. */
export async function fetchAccounts(): Promise<ActionResult<TempoAccount[]>> {
  return runAction(() => daemonListAccounts());
}

/**
 * Create a Jira issue (setting its Tempo account so worklogs map to a
 * customer), then assign it to the block in one round-trip. The created
 * ticket is cached daemon-side, so the picker can render it afterwards.
 */
export async function createTicket(
  input: CreateTicketInput,
  blockId: number,
  day: string,
): Promise<ActionResult<JiraTicket>> {
  return runAction(async () => {
    const ticket = await daemonCreateTicket(input);
    await daemonAssignTicket(blockId, ticket.key);
    return ticket;
  }, `/${day}`);
}

// ───────────────────────── billing export ─────────────────────────

/**
 * Compute the day's billing line items. Read-only, so no `revalidateOn` —
 * mirrors `fetchProjects`/`fetchBlockEvents`. The panel calls this on
 * open and after marking.
 */
export async function exportBilling(
  day: string,
): Promise<ActionResult<ExportResponse>> {
  return runAction(() => daemonExportBilling(day));
}

/**
 * Mark the day's blocks as exported/billed. A mutation — revalidates the
 * day page so any surface showing export state refreshes.
 */
export async function markExported(
  day: string,
): Promise<ActionResult<MarkExportResponse>> {
  return runAction(() => daemonMarkExported(day), `/${day}`);
}

// ─────────────────── billing registry (Settings → Billing) ───────────────────

/** The registry + unmapped-folder discovery. Read-only, no revalidate. */
export async function fetchBillingRegistry(): Promise<ActionResult<BillingRegistry>> {
  return runAction(() => daemonLoadBillingRegistry());
}

/**
 * Registry mutations revalidate the day page because changing a mapping
 * changes how that day's export resolves.
 */
export async function saveBillingCustomer(
  customer: BillingCustomer,
  day: string,
): Promise<ActionResult<{ id: number }>> {
  return runAction(() => daemonSaveBillingCustomer(customer), `/${day}`);
}

export async function deleteBillingCustomer(
  id: number,
  day: string,
): Promise<ActionResult<{ removed: boolean }>> {
  return runAction(() => daemonDeleteBillingCustomer(id), `/${day}`);
}

export async function saveBillingFolder(
  folder: BillingFolderMap,
  day: string,
): Promise<ActionResult<{ id: number }>> {
  return runAction(() => daemonSaveBillingFolder(folder), `/${day}`);
}

export async function deleteBillingFolder(
  id: number,
  day: string,
): Promise<ActionResult<{ removed: boolean }>> {
  return runAction(() => daemonDeleteBillingFolder(id), `/${day}`);
}

// ───────────────────────── settings ─────────────────────────

/** Load the settings snapshot for the panel. Read-only, no revalidate. */
export async function fetchSettings(): Promise<ActionResult<SettingsView>> {
  return runAction(() => daemonLoadSettings());
}

/**
 * Persist a partial settings update. Revalidates the current day so a
 * classification change (which reclassifies blocks) is reflected without
 * a manual reload. `day` is the page the user saved from.
 */
export async function saveSettings(
  update: SettingsUpdate,
  day: string,
): Promise<ActionResult<SettingsSaveResponse>> {
  return runAction(() => daemonSaveSettings(update), `/${day}`);
}

// Exported for tests. Not used by callers — they use the CRUD/query
// wrappers above. Exposing the helper lets us verify the happy path
// AND the throws-inside-fn and throws-inside-revalidate branches.
export { runAction as _runActionForTests };
