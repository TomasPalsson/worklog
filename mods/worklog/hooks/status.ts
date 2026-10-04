import { atom, read, update } from 'claude-code'
import type { EngineInterface, On } from 'claude-code'
import { DEFAULT_REQUIRED_SECONDS, NUDGE_STORE_PREFIX, POLL_MS } from './contract'
import type { DaySummary, Io, WeekCloseout } from './contract'
import {
  daemonGet,
  daemonToday,
  formatHours,
  lastWorkday,
  mondayOf,
  workContext,
  workSeconds,
} from './lib'

const hours = atom(
  { plugin: 'worklog', key: 'hours' } as const,
  '',
)

async function makeIo($: EngineInterface): Promise<Io> {
  return {
    fetch: (url, init) => $.http.fetch(url, init),
    run: (argv, cwd) => $.process.run(argv, { cwd }),
    after: (milliseconds, callback) => $.clock.after(milliseconds, callback),
    now: () => $.clock.now(),
    home: await $.env.get('HOME'),
  }
}

export function registerStatus(on: On): void {
  on('session.start', async ($, event, next) => {
    const io = await makeIo($)

    async function showHours(): Promise<void> {
      const today = await daemonToday(io)
      const summary = today.ok ? await daemonGet<DaySummary>(io, `/days/${today.value}`) : today
      const text = summary.ok ? `worklog ${formatHours(workSeconds(summary.value.blocks))}` : ''
      await update($, hours, () => text)
    }

    async function remind(): Promise<void> {
      const context = await workContext(io, event.cwd)
      const today = await daemonToday(io)
      if (!context.isWork || !today.ok) return
      const marker = `${NUDGE_STORE_PREFIX}${today.value}`
      if (await $.store.get(marker)) return
      const previous = lastWorkday(today.value)
      const week = await daemonGet<WeekCloseout>(io, `/weeks/${mondayOf(previous)}/closeout`)
      const day = week.ok ? week.value.days.find(candidate => candidate.day === previous) : undefined
      if (day === undefined) return
      const required = day.required_seconds ?? DEFAULT_REQUIRED_SECONDS
      if (required === 0) return
      const problems: string[] = []
      if (day.pending_lines > 0) problems.push(`${day.pending_lines} unsynced lines`)
      const worked = Math.max(day.logged_seconds, day.tempo_seconds)
      if (worked < required) {
        problems.push(`${formatHours(worked)} logged of ${formatHours(required)}`)
      }
      if (problems.length === 0) return
      await $.ui.toast(`worklog: ${previous} has ${problems.join(' and ')}`)
      await $.store.set(marker, true)
    }

    void showHours()
    void remind()
    $.clock.every(POLL_MS, () => {
      void showHours()
    })
    return next(event)
  })

  on('ui.render', { component: 'PromptHint' }, async ($, event, next) => {
    const text = await read($, hours)
    return next(text ? { ...event, props: { ...event.props, tail: event.props.tail ? `${event.props.tail} · ${text}` : text } } : event)
  })
}
