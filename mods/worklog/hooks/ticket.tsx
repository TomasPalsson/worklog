import { atom, read, update } from 'claude-code'
import type { EngineInterface, On } from 'claude-code'
import { JIRA_KEY_RE, MORE_TICKETS, TICKET_ASKED_STORE_PREFIX, TICKET_PANE_ID } from './contract'
import type { Io, RecentTask } from './contract'
import { cut, daemonGet, filterPickerOptions, keyOfChoice, ticketChoices, ticketPickerOptions, workContext } from './lib'

const TASKS_CAP_MS = 1500
const PICK_CAP_MS = 8000
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

type TicketState = {
  cwd: string | undefined
  ticket: string | undefined
  askable: boolean
  recorded: string | undefined
  handoff: string | undefined
}

const picker = atom({ plugin: 'worklog', key: 'picker' } as const, {
  options: [] as { value: string; label: string }[],
  query: '',
})

const handoffFor = (answer: string, id: string): string =>
  answer === 'Create a new ticket'
    ? `Instruction "create a ticket": the Owner wants a new Jira ticket for this session. Once the first request makes the task clear, create it (worklog skill, Jira recipe, one confirm), then run worklog ticket use <KEY> --session ${id}.`
    : `Instruction "find the ticket for: ${answer}": run worklog ticket find, confirm the match with the Owner, then run worklog ticket use <KEY> --session ${id}.`

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

type Pick = { picked: string | undefined; likely: string | undefined }

// Verdict reads the first prompt; any failure means "no pick" and the Owner is asked.
async function verdictPick($: EngineInterface, cwd: string, id: string, ticket: string | undefined, text: string): Promise<Pick> {
  const none: Pick = { picked: undefined, likely: undefined }
  let timer: { cancel: () => void } | undefined
  const cap = new Promise<undefined>(resolve => {
    timer = $.clock.after(PICK_CAP_MS, () => resolve(undefined))
  })
  try {
    const argv = ['worklog', '--json', 'ticket', 'pick', '--session', id, ...(ticket ? ['--also', ticket] : []), '--', text]
    const result = await Promise.race([$.process.run(argv, { cwd }), cap])
    if (result === undefined || result.exitCode !== 0) return none
    const body = JSON.parse(result.stdout.trim()) as { picked?: unknown; likely?: unknown }
    const key = (value: unknown) => (typeof value === 'string' && EXACT_KEY_RE.test(value) ? value : undefined)
    return { picked: key(body.picked), likely: key(body.likely) }
  } catch {
    return none
  } finally {
    timer?.cancel()
  }
}

async function openPicker(
  $: EngineInterface,
  state: TicketState,
  tasks: RecentTask[],
  likely?: string,
): Promise<void> {
  await update($, picker, () => ({ options: ticketPickerOptions(state.ticket, tasks, likely), query: '' }))
  const opened = await $.ui.open({ id: TICKET_PANE_ID, title: 'worklog ticket', focus: true, closeOnEscape: true })
  if (!opened.isPlaced) await $.ui.log('worklog: widen the terminal or run /wl ticket')
}

async function recordKey($: EngineInterface, cwd: string, state: TicketState, key: string, id: string): Promise<boolean> {
  try {
    const result = await $.process.run(['worklog', 'ticket', 'use', key, '--session', id], { cwd })
    if (result.exitCode !== 0) throw new Error(result.stderr.trim())
    state.recorded = key
    state.handoff = undefined
    await $.ui.toast(`worklog: ${key}`)
    return true
  } catch (error) {
    await $.ui.log(`worklog: could not record ${key} — ${error instanceof Error ? error.message : String(error)}`)
    return false
  }
}

async function choose($: EngineInterface, state: TicketState, value: string): Promise<void> {
  await $.ui.close({ id: TICKET_PANE_ID })
  const id = await $.session.id()
  // a pick before the first prompt must stop askTicket asking again
  await $.store.set(`${TICKET_ASKED_STORE_PREFIX}${id}`, value)
  let note: string | undefined
  if (value === 'create') note = state.handoff = handoffFor('Create a new ticket', id)
  else if (state.cwd !== undefined && (await recordKey($, state.cwd, state, value, id))) {
    note = `Session ticket: ${value} (the Owner picked it in the worklog ticket picker).`
  }
  // prompt.context was cached at the first prompt, so a pick from the pane is told as a row now
  if (note !== undefined) {
    await $.session.append({ message: { type: 'user', content: [{ type: 'text', text: note }] } }).catch(() => undefined)
  }
}

async function askTicket($: EngineInterface, cwd: string, state: TicketState, text: string): Promise<void> {
  const id = await $.session.id()
  const marker = `${TICKET_ASKED_STORE_PREFIX}${id}`
  if (await $.store.get(marker)) return
  const { picked, likely } = await verdictPick($, cwd, id, state.ticket, text)
  if (picked !== undefined) {
    state.recorded = picked
    state.handoff = undefined
    await $.store.set(marker, picked)
    await $.ui.toast(`worklog: ${picked} (Verdict)`)
    return
  }
  const tasks = await recentTasks(await makeIo($))
  const options = ticketChoices(state.ticket, tasks, likely).map(cut)
  let answer: string
  try {
    answer = await $.ui.ask('Which Jira ticket is this session for?', { options, header: 'Ticket' })
  } catch {
    await $.store.set(marker, true)
    await $.ui.log(`worklog: no ticket question shown — run worklog ticket use KEY --session ${id}`)
    return
  }
  await $.store.set(marker, answer)
  if (answer === MORE_TICKETS) return openPicker($, state, tasks, likely)
  const key = options.includes(answer) ? keyOfChoice(answer) : EXACT_KEY_RE.exec(answer)?.[1]
  if (key === undefined) {
    if (answer !== 'Skip') state.handoff = handoffFor(answer, id)
    return
  }
  await recordKey($, cwd, state, key, id)
}

export function registerTicket(on: On): void {
  const state: TicketState = { cwd: undefined, ticket: undefined, askable: false, recorded: undefined, handoff: undefined }

  on('session.start', { surface: 'terminal' }, async ($, event, next) => {
    state.cwd = event.cwd
    const isWork = await refresh($, state)
    if (state.ticket !== undefined) await $.ui.toast(`worklog: ${state.ticket}`)
    state.askable = isWork && event.isInteractive
    return next(event)
  })

  on('command.run', { command: 'wl' }, async ($, event, next) => {
    if (event.args.trim() !== 'ticket') return next(event)
    if (!state.askable || state.cwd === undefined) return { text: 'worklog ticket: not a work session' }
    await openPicker($, state, await recentTasks(await makeIo($)))
    return { text: 'worklog ticket picker opened' }
  })

  on('ui.render', { component: 'Pane', requestId: TICKET_PANE_ID }, async ($, event) => {
    const { Box, Input, Select } = $.ui.resolve(event)
    const { options, query } = await read($, picker)
    const shown = filterPickerOptions(options, query)
    return (
      <Box flexDirection="column" borderStyle="round" borderDimColor paddingX={1}>
        <Input
          key="filter"
          autoFocus
          placeholder="type to filter, Enter picks the first"
          value={query}
          submitLabel="pick"
          onInput={value => update($, picker, current => ({ ...current, query: value }))}
          onSubmit={async value => {
            const first = filterPickerOptions(options, value).find(option => option.value !== 'create')
            if (first !== undefined) await choose($, state, first.value)
          }}
        />
        <Select key="ticket" options={shown} onSelect={value => choose($, state, value)} />
      </Box>
    )
  })

  on('session.end', async ($, event, next) => {
    if (event.reason === 'clear') {
      state.recorded = undefined
      state.handoff = undefined
    }
    return next(event)
  })

  // The first prompt (and the first after /clear) picks or asks the ticket before it enters.
  // The hand-off rides on each prompt: prompt.context is computed once and cached, so it would never carry it.
  on('prompt.submit', async ($, event, next) => {
    if (state.askable && state.cwd !== undefined) await askTicket($, state.cwd, state, event.text).catch(() => undefined)
    return next(state.handoff === undefined ? event : { ...event, context: [...(event.context ?? []), state.handoff] })
  })

  on('prompt.context', async ($, event, next) => {
    const result = await next(event)
    const ticket = state.recorded ?? state.ticket
    const text = state.recorded === undefined ? `Current branch ticket: ${ticket}` : `Session ticket: ${ticket}`
    return ticket === undefined ? result : { ...result, blocks: [...result.blocks, { name: 'worklog', text }] }
  })

  for (const kind of ['commit', 'pr'] as const) {
    on('attribution.text', { kind }, async ($, event, next) => {
      const result = await next(event)
      const ticket = state.recorded ?? state.ticket
      return ticket === undefined ? result : { ...result, text: `${result.text}\n\nTicket: ${ticket}` }
    })
  }

  on('tool.call', { tool: 'Bash' }, async ($, event, next) => {
    const result = await next(event)
    if (result.deny !== undefined || result.isError === true) return result
    const [, key, id] = /worklog ticket use (\S+) --session (\S+)/.exec(event.command) ?? []
    if (key !== undefined && EXACT_KEY_RE.test(key) && id === (await $.session.id())) {
      state.recorded = key
      state.handoff = undefined
    }
    return result
  })

  on('turn.complete', async ($, event, next) => {
    void refresh($, state)
    return next(event)
  })
}
