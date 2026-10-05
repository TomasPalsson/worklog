import type { EngineInterface, On } from 'claude-code'
import { JIRA_KEY_RE, TICKET_ASKED_STORE_PREFIX } from './contract'
import type { Io, RecentTask } from './contract'
import { daemonGet, keyOfChoice, ticketChoices, workContext } from './lib'

const TASKS_CAP_MS = 1500
const LABEL_MAX = 60
const EXACT_KEY_RE = new RegExp(`^${JIRA_KEY_RE.source}$`)

async function makeIo($: EngineInterface): Promise<Io> {
  return {
    fetch: (url, init) => $.http.fetch(url, init),
    run: (argv, cwd) => $.process.run(argv, { cwd }),
    after: (milliseconds, callback) => $.clock.after(milliseconds, callback),
    now: () => $.clock.now(),
    home: await $.env.get('HOME'),
  }
}

type TicketState = { cwd: string | undefined; ticket: string | undefined }

async function refresh($: EngineInterface, state: TicketState): Promise<boolean> {
  if (state.cwd === undefined) return false
  const context = await workContext(await makeIo($), state.cwd)
  state.ticket = context.isWork ? context.ticket : undefined
  return context.isWork
}

async function recentTasks(io: Io): Promise<RecentTask[]> {
  let timer: { cancel: () => void } | undefined
  const cap = new Promise<undefined>(resolve => {
    timer = io.after(TASKS_CAP_MS, () => resolve(undefined))
  })
  try {
    const result = await Promise.race([daemonGet<{ tasks: RecentTask[] }>(io, '/tasks'), cap])
    return result?.ok && Array.isArray(result.value.tasks) ? result.value.tasks : []
  } finally {
    timer?.cancel()
  }
}

const cut = (label: string): string =>
  label.length > LABEL_MAX ? `${label.slice(0, LABEL_MAX - 1)}…` : label

async function askTicket($: EngineInterface, cwd: string, branchTicket: string | undefined): Promise<void> {
  const id = await $.session.id()
  const marker = `${TICKET_ASKED_STORE_PREFIX}${id}`
  if (await $.store.get(marker)) return
  const options = ticketChoices(branchTicket, await recentTasks(await makeIo($))).map(cut)
  let answer: string
  try {
    answer = await $.ui.ask('Which Jira ticket is this session for?', { options, header: 'Ticket' })
  } catch {
    await $.store.set(marker, true)
    await $.ui.log(`worklog: no ticket question shown — run worklog ticket use KEY --session ${id}`)
    return
  }
  await $.store.set(marker, answer)
  const key = options.includes(answer) ? keyOfChoice(answer) : EXACT_KEY_RE.exec(answer)?.[1]
  if (key === undefined) return
  try {
    const result = await $.process.run(['worklog', 'ticket', 'use', key, '--session', id], { cwd })
    if (result.exitCode !== 0) throw new Error(result.stderr.trim())
    await $.ui.toast(`worklog: ${key}`)
  } catch (error) {
    await $.ui.log(`worklog: could not record ${key} — ${error instanceof Error ? error.message : String(error)}`)
  }
}

export function registerTicket(on: On): void {
  const state: TicketState = { cwd: undefined, ticket: undefined }

  on('session.start', { surface: 'terminal' }, async ($, event, next) => {
    state.cwd = event.cwd
    const isWork = await refresh($, state)
    if (state.ticket !== undefined) await $.ui.toast(`worklog: ${state.ticket}`)
    if (isWork && event.isInteractive) void askTicket($, event.cwd, state.ticket).catch(() => undefined)
    return next(event)
  })

  on('prompt.context', async ($, event, next) => {
    const result = await next(event)
    if (state.ticket === undefined) return result
    return {
      ...result,
      blocks: [...result.blocks, { name: 'worklog', text: `Current branch ticket: ${state.ticket}` }],
    }
  })

  for (const kind of ['commit', 'pr'] as const) {
    on('attribution.text', { kind }, async ($, event, next) => {
      const result = await next(event)
      return state.ticket === undefined ? result : { ...result, text: `${result.text}\n\nTicket: ${state.ticket}` }
    })
  }

  on('turn.complete', async ($, event, next) => {
    void refresh($, state)
    return next(event)
  })
}
