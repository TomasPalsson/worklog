import { test, expect, mock } from 'claude-code/testing'
import type { On } from 'claude-code'
import type { Block, CloseoutDay } from './contract'

const SESSION = { surface: 'terminal', isInteractive: true, cwd: '/Users/me/Desktop/Work/app' } as const
const TODAY = '2026-10-05'
const LAST_WORKDAY = '2026-10-02'

const block = (overrides: Partial<Block>) => ({
  id: 1,
  day: TODAY,
  jira_issue: null,
  started_at: '2026-10-05T08:00:00Z',
  ended_at: '2026-10-05T09:00:00Z',
  duration_seconds: 3600,
  description: null,
  is_personal: false,
  ignored_at: null,
  tempo_worklog_id: null,
  exported_at: null,
  project: null,
  ...overrides,
})

const hint = (tail?: string) => ({
  component: 'PromptHint',
  surface: 'terminal',
  viewport: { columns: 120, rows: 40, isFullscreen: false },
  props: { isDraft: false, isWorking: false, hint: '? for shortcuts', ...(tail === undefined ? {} : { tail }) },
}) as const

const closeoutDay = (overrides: Partial<CloseoutDay>): CloseoutDay => ({
  day: LAST_WORKDAY,
  logged_seconds: 8 * 3600,
  synced_seconds: 8 * 3600,
  tempo_seconds: 8 * 3600,
  required_seconds: 8 * 3600,
  pending_lines: 0,
  ...overrides,
})

type Options = {
  blocks?: ReturnType<typeof block>[]
  closeoutDays?: CloseoutDay[]
  isDown?: boolean
  marker?: boolean
}

function world(on: On, options: Options = {}) {
  const requested: string[] = []
  const toasts: string[] = []
  const stored: { key: string; value: unknown }[] = []
  const answer = (body: unknown) => ({ value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } })

  on('session.start', ($, event) => ({ cwd: event.cwd }))
  on('env.get', () => ({ value: '/Users/me' }))
  on('process.run', () => ({ value: { exitCode: 1, stdout: '' } }))
  on('http.fetch', ($, event) => {
    requested.push(event.url)
    if (options.isDown) return { deny: 'connection refused' }
    if (event.url.includes('/logged')) return answer({ today: TODAY })
    if (event.url.includes('/days/')) return answer({ day: TODAY, total_seconds: 0, blocks: options.blocks ?? [] })
    return answer({ days: options.closeoutDays ?? [closeoutDay({})] })
  })
  on('store.get', ($, event) => ({
    value: options.marker && event.key === `nudge:${TODAY}` ? true : undefined,
  }))
  on('store.set', ($, event) => {
    stored.push({ key: event.key, value: event.value })
    return { value: undefined }
  })
  on('ui.toast', ($, event) => {
    toasts.push(event.text)
    return { value: undefined }
  })
  on('ui.render', { component: 'PromptHint' }, ($, event) => ({
    type: 'Text',
    children: [event.props.tail ?? ''],
  }))
  return { requested, toasts, stored }
}

test('session start puts on the prompt hint tail worklog hours counting only non-personal, non-ignored blocks', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, {
    blocks: [
      block({ duration_seconds: 3600 }),
      block({ duration_seconds: 1800 }),
      block({ duration_seconds: 900, is_personal: true }),
      block({ duration_seconds: 600, ignored_at: '2026-10-05T10:00:00Z' }),
    ],
  })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(JSON.stringify(await $.ui.render(hint()))).toContain('"worklog 1h30"')
  expect(seen.requested).toContain(`http://127.0.0.1:9323/days/${TODAY}`)
})

test('the prompt hint tail refreshes every 60 seconds', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { blocks: [block({ duration_seconds: 3600 })] })
  await $.session.start(SESSION)
  await clock.advance(1000)
  const before = seen.requested.length
  await clock.advance(60_000)
  expect(seen.requested.length).toBeGreaterThan(before)
  expect(JSON.stringify(await $.ui.render(hint()))).toContain('"worklog 1h00"')
})

test('an existing tail is kept and the hours are appended after a separator', async ($, on) => {
  const clock = mock.clock(on)
  world(on, { blocks: [block({ duration_seconds: 3600 }), block({ duration_seconds: 1800 })] })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(JSON.stringify(await $.ui.render(hint('x')))).toContain('"x · worklog 1h30"')
})

test('a down daemon leaves the prompt hint tail absent and shows no error text', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { isDown: true })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(JSON.stringify(await $.ui.render(hint()))).toBe('{"type":"Text","children":[""]}')
  expect(JSON.stringify(await $.ui.render(hint('x')))).toContain('"x"')
  expect(seen.toasts).toEqual([])
})

test('unsynced lines on the last workday toast once and store the marker', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { closeoutDays: [closeoutDay({ pending_lines: 3 })] })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toHaveLength(1)
  expect(seen.toasts[0]).toContain('3 unsynced')
  expect(seen.stored).toEqual([{ key: `nudge:${TODAY}`, value: true }])
  expect(seen.requested).toContain('http://127.0.0.1:9323/weeks/2026-09-28/closeout')
})

test('a short day toasts the hours against the required hours', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { closeoutDays: [closeoutDay({ logged_seconds: 6 * 3600 })] })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toHaveLength(1)
  expect(seen.toasts[0]).toContain('6h00')
  expect(seen.toasts[0]).toContain('8h00')
})

test('unsynced and short together make one toast naming both', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, {
    closeoutDays: [closeoutDay({ logged_seconds: 6 * 3600, pending_lines: 2 })],
  })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toHaveLength(1)
  expect(seen.toasts[0]).toContain('2 unsynced')
  expect(seen.toasts[0]).toContain('6h00')
})

test('a complete synced day gets no reminder', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on)
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toEqual([])
  expect(seen.stored).toEqual([])
})

test('required seconds null means 8h, so 7h logged is short', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, {
    closeoutDays: [closeoutDay({ logged_seconds: 7 * 3600, required_seconds: null })],
  })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toHaveLength(1)
  expect(seen.toasts[0]).toContain('8h00')
})

test('required seconds 0 means no reminder', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, {
    closeoutDays: [closeoutDay({ logged_seconds: 0, required_seconds: 0 })],
  })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toEqual([])
})

test('required seconds 28800 with 8h logged means no reminder', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { closeoutDays: [closeoutDay({ required_seconds: 28800 })] })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toEqual([])
})

test('an existing marker for today suppresses the toast in a later session', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { closeoutDays: [closeoutDay({ pending_lines: 3 })], marker: true })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toEqual([])
  expect(seen.stored).toEqual([])
})

test('a down daemon shows no reminder and stores no marker', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { isDown: true })
  await $.session.start(SESSION)
  await clock.advance(1000)
  expect(seen.toasts).toEqual([])
  expect(seen.stored).toEqual([])
})

test('outside a work folder there is no reminder', async ($, on) => {
  const clock = mock.clock(on)
  const seen = world(on, { closeoutDays: [closeoutDay({ pending_lines: 3 })] })
  await $.session.start({ ...SESSION, cwd: '/Users/me/code/play' })
  await clock.advance(1000)
  expect(seen.toasts).toEqual([])
})
