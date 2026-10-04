import type { EngineInterface, On } from 'claude-code'
import type { Io } from './contract'
import { workContext } from './lib'

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

async function refresh($: EngineInterface, state: TicketState): Promise<void> {
  if (state.cwd === undefined) return
  const context = await workContext(await makeIo($), state.cwd)
  state.ticket = context.isWork ? context.ticket : undefined
}

export function registerTicket(on: On): void {
  const state: TicketState = { cwd: undefined, ticket: undefined }

  on('session.start', { surface: 'terminal' }, async ($, event, next) => {
    state.cwd = event.cwd
    await refresh($, state)
    if (state.ticket !== undefined) await $.ui.toast(`worklog: ${state.ticket}`)
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
