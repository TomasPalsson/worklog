import { DAEMON_URL, JIRA_KEY_RE, MORE_TICKETS } from './contract'
import type {
  Block,
  DaemonResult,
  Io,
  LocalDay,
  RecentTask,
  ReviewAction,
  WorkContext,
} from './contract'

const TIMEOUT_MS = 2000
const DAY_MS = 24 * 3600 * 1000
const WORK_ORIGIN_RE = /github\.com[:/]aproorg\//

function shiftDay(day: LocalDay, days: number): LocalDay {
  return new Date(Date.parse(`${day}T00:00:00Z`) + days * DAY_MS).toISOString().slice(0, 10)
}

function weekdayOf(day: LocalDay): number {
  return new Date(`${day}T00:00:00Z`).getUTCDay()
}

export function lastWorkday(today: LocalDay): LocalDay {
  const weekday = weekdayOf(today)
  if (weekday === 1) return shiftDay(today, -3)
  if (weekday === 0) return shiftDay(today, -2)
  return shiftDay(today, -1)
}

export function mondayOf(day: LocalDay): LocalDay {
  return shiftDay(day, -((weekdayOf(day) + 6) % 7))
}

export function workSeconds(blocks: readonly Block[]): number {
  return blocks
    .filter(block => !block.is_personal && block.ignored_at === null)
    .reduce((total, block) => total + block.duration_seconds, 0)
}

export function formatHours(seconds: number): string {
  const minutes = Math.round(seconds / 60)
  return `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, '0')}`
}

export function ticketFromBranch(branch: string | undefined): string | undefined {
  return branch?.match(JIRA_KEY_RE)?.[1]
}

const LABEL_MAX = 60

export const cut = (label: string): string =>
  label.length > LABEL_MAX ? `${label.slice(0, LABEL_MAX - 1)}…` : label

const leadKeys = (branchTicket: string | undefined, likely: string | undefined): string[] =>
  [likely, branchTicket !== likely ? branchTicket : undefined].filter(
    (key): key is string => key !== undefined,
  )

const newestFirst = (tasks: readonly RecentTask[], skip: readonly string[]): RecentTask[] =>
  tasks
    .filter((task) => task.assigned && !skip.includes(task.key))
    .sort((a, b) => (b.last_worked_day ?? '').localeCompare(a.last_worked_day ?? ''))

export function ticketChoices(
  branchTicket: string | undefined,
  recentTasks: readonly RecentTask[],
  likely?: string,
): string[] {
  const label = (key: string) => {
    const task = recentTasks.find((t) => t.key === key)
    return task ? `${key} ${task.summary}` : key
  }
  const lead = leadKeys(branchTicket, likely)
  const candidates = newestFirst(recentTasks, lead)
  const recent = candidates.slice(0, Math.max(0, 2 - lead.length))
  return [
    ...lead.map((key) => (key === likely ? label(key) : key)),
    ...recent.map((task) => `${task.key} ${task.summary}`),
    candidates.length > recent.length ? MORE_TICKETS : 'Create a new ticket',
    'Skip',
  ]
}

export function ticketPickerOptions(
  branchTicket: string | undefined,
  recentTasks: readonly RecentTask[],
  likely?: string,
): { value: string; label: string }[] {
  const label = (key: string) => {
    const task = recentTasks.find((t) => t.key === key)
    return cut(task ? `${key} ${task.summary}` : key)
  }
  const lead = leadKeys(branchTicket, likely)
  return [
    ...lead.map((key) => ({ value: key, label: label(key) })),
    ...newestFirst(recentTasks, lead).map((task) => ({ value: task.key, label: label(task.key) })),
    { value: 'create', label: 'Create a new ticket' },
  ]
}

// Every word of the query must appear in the label, any case; 'create' always stays.
export function filterPickerOptions(
  options: readonly { value: string; label: string }[],
  query: string,
): { value: string; label: string }[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  return options.filter(
    (option) =>
      option.value === 'create' || words.every((word) => option.label.toLowerCase().includes(word)),
  )
}

export function keyOfChoice(label: string): string | undefined {
  return label.match(new RegExp(`^${JIRA_KEY_RE.source}(?:\\s|$)`))?.[1]
}

export function reviewRequest(
  blockId: number,
  action: ReviewAction,
): { path: string; body: unknown } {
  const path = `/blocks/${blockId}`
  switch (action.kind) {
    case 'personal':
      return { path: `${path}/personal`, body: { is_personal: action.is_personal } }
    case 'ignore':
      return { path: `${path}/ignore`, body: { ignored: action.ignored } }
    case 'ticket':
      return { path: `${path}/ticket`, body: { jira_issue: action.jira_issue } }
    case 'description':
      return { path: `${path}/description`, body: { description: action.description } }
  }
}

async function daemonRequest<T>(
  io: Io,
  path: string,
  init?: { method: string; headers: Record<string, string>; body: string },
): Promise<DaemonResult<T>> {
  const request = async (): Promise<DaemonResult<T>> => {
    try {
      const response = await io.fetch(`${DAEMON_URL}${path}`, init)
      let body: unknown
      try {
        body = JSON.parse(response.text)
      } catch {
        body = undefined
      }
      if (response.ok && body !== undefined) return { ok: true, value: body as T }
      const message = (body as { error?: unknown } | undefined)?.error
      return { ok: false, error: typeof message === 'string' ? message : `HTTP ${response.status}` }
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) }
    }
  }
  let timer: { cancel: () => void } | undefined
  const timeout = new Promise<DaemonResult<T>>(resolve => {
    timer = io.after(TIMEOUT_MS, () => resolve({ ok: false, error: 'timeout' }))
  })
  try {
    return await Promise.race([request(), timeout])
  } finally {
    timer?.cancel()
  }
}

export function daemonGet<T>(io: Io, path: string): Promise<DaemonResult<T>> {
  return daemonRequest<T>(io, path)
}

export function daemonPost<T>(io: Io, path: string, body: unknown): Promise<DaemonResult<T>> {
  return daemonRequest<T>(io, path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
}

export async function daemonToday(io: Io): Promise<DaemonResult<LocalDay>> {
  const utcDay = new Date(await io.now()).toISOString().slice(0, 10)
  const result = await daemonGet<{ today: LocalDay }>(io, `/logged?from=${utcDay}&to=${utcDay}`)
  return result.ok ? { ok: true, value: result.value.today } : result
}

async function gitOutput(io: Io, cwd: string, argv: string[]): Promise<string | undefined> {
  try {
    const result = await io.run(['git', ...argv], cwd)
    return result.exitCode === 0 ? result.stdout.trim() : undefined
  } catch {
    return undefined
  }
}

export async function workContext(io: Io, cwd: string): Promise<WorkContext> {
  const workFolder = io.home === undefined ? undefined : `${io.home}/Desktop/Work`
  const inWorkFolder =
    workFolder !== undefined && (cwd === workFolder || cwd.startsWith(`${workFolder}/`))
  const branch = (await gitOutput(io, cwd, ['branch', '--show-current'])) || undefined
  const origin = inWorkFolder ? undefined : await gitOutput(io, cwd, ['remote', 'get-url', 'origin'])
  return {
    cwd,
    isWork: inWorkFolder || WORK_ORIGIN_RE.test(origin ?? ''),
    branch,
    ticket: ticketFromBranch(branch),
  }
}
