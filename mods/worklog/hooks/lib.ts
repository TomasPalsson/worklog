import type { EngineInterface } from 'claude-code'
import { DAEMON_URL, JIRA_KEY_RE } from './contract'
import type {
  Block,
  DaemonResult,
  LocalDay,
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
  $: EngineInterface,
  path: string,
  init?: { method: string; headers: Record<string, string>; body: string },
): Promise<DaemonResult<T>> {
  const request = async (): Promise<DaemonResult<T>> => {
    try {
      const response = await $.http.fetch(`${DAEMON_URL}${path}`, init)
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
    timer = $.clock.after(TIMEOUT_MS, () => resolve({ ok: false, error: 'timeout' }))
  })
  try {
    return await Promise.race([request(), timeout])
  } finally {
    timer?.cancel()
  }
}

export function daemonGet<T>($: EngineInterface, path: string): Promise<DaemonResult<T>> {
  return daemonRequest<T>($, path)
}

export function daemonPost<T>(
  $: EngineInterface,
  path: string,
  body: unknown,
): Promise<DaemonResult<T>> {
  return daemonRequest<T>($, path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
}

export async function daemonToday($: EngineInterface): Promise<DaemonResult<LocalDay>> {
  const utcDay = new Date(await $.clock.now()).toISOString().slice(0, 10)
  const result = await daemonGet<{ today: LocalDay }>($, `/logged?from=${utcDay}&to=${utcDay}`)
  return result.ok ? { ok: true, value: result.value.today } : result
}

async function gitOutput(
  $: EngineInterface,
  cwd: string,
  argv: string[],
): Promise<string | undefined> {
  try {
    const result = await $.process.run(['git', ...argv], { cwd })
    return result.exitCode === 0 ? result.stdout.trim() : undefined
  } catch {
    return undefined
  }
}

export async function workContext($: EngineInterface, cwd: string): Promise<WorkContext> {
  const home = await $.env.get('HOME')
  const workFolder = home === undefined ? undefined : `${home}/Desktop/Work`
  const inWorkFolder =
    workFolder !== undefined && (cwd === workFolder || cwd.startsWith(`${workFolder}/`))
  const branch = (await gitOutput($, cwd, ['branch', '--show-current'])) || undefined
  const origin = inWorkFolder ? undefined : await gitOutput($, cwd, ['remote', 'get-url', 'origin'])
  return {
    cwd,
    isWork: inWorkFolder || WORK_ORIGIN_RE.test(origin ?? ''),
    branch,
    ticket: ticketFromBranch(branch),
  }
}
