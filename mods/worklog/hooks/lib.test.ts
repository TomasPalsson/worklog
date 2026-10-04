import { test, expect } from 'claude-code/testing'
import type { EngineInterface } from 'claude-code'
import {
  daemonGet,
  daemonPost,
  daemonToday,
  formatHours,
  lastWorkday,
  mondayOf,
  reviewRequest,
  ticketFromBranch,
  workContext,
  workSeconds,
} from './lib'
import type { Block } from './contract'

const block = (overrides: Partial<Block>): Block => ({
  id: 1,
  day: '2026-10-05',
  jira_issue: null,
  started_at: '2026-10-05T08:00:00Z',
  ended_at: '2026-10-05T09:00:00Z',
  duration_seconds: 3600,
  description: null,
  is_personal: false,
  ignored_at: null,
  tempo_worklog_id: null,
  exported_at: null,
  ...overrides,
})

type Fetched = { url: string; init?: { method?: string; body?: string } }
type Fake = {
  engine: EngineInterface
  fetched: Fetched[]
  fireTimeout: () => void
}

// A hand-built `$`: `$` cannot cross an import inside a plugin, so lib.ts is exercised directly.
const fakeEngine = (options: {
  fetch?: (url: string, init?: Fetched['init']) => Promise<unknown>
  git?: Record<string, string>
  home?: string
  now?: number
}): Fake => {
  const fetched: Fetched[] = []
  let timeoutFunction: (() => void) | undefined
  const engine = {
    http: {
      fetch: (url: string, init?: Fetched['init']) => {
        fetched.push({ url, init })
        return options.fetch?.(url, init)
      },
    },
    process: {
      run: async (argv: string[]) => {
        const stdout = options.git?.[argv.join(' ')]
        return { exitCode: stdout === undefined ? 1 : 0, stdout: stdout ?? '', stderr: '' }
      },
    },
    env: { get: async () => options.home },
    clock: {
      now: async () => options.now ?? 0,
      after: (_milliseconds: number, callback: () => void) => {
        timeoutFunction = callback
        return { cancel: () => (timeoutFunction = undefined) }
      },
    },
  } as unknown as EngineInterface
  return { engine, fetched, fireTimeout: () => timeoutFunction?.() }
}

const reply = (status: number, text: string) =>
  Promise.resolve({ status, ok: status >= 200 && status < 300, headers: {}, text })

test('lastWorkday: Monday maps to Friday, Sunday to Friday, midweek to the day before', () => {
  expect(lastWorkday('2026-10-05')).toBe('2026-10-02')
  expect(lastWorkday('2026-10-04')).toBe('2026-10-02')
  expect(lastWorkday('2026-10-03')).toBe('2026-10-02')
  expect(lastWorkday('2026-10-07')).toBe('2026-10-06')
  expect(lastWorkday('2026-03-01')).toBe('2026-02-27')
})

test('mondayOf: any day of the week maps to its Monday', () => {
  expect(mondayOf('2026-10-05')).toBe('2026-10-05')
  expect(mondayOf('2026-10-07')).toBe('2026-10-05')
  expect(mondayOf('2026-10-11')).toBe('2026-10-05')
  expect(mondayOf('2026-03-01')).toBe('2026-02-23')
})

test('workSeconds counts only blocks that are neither personal nor ignored', () => {
  const blocks = [
    block({ duration_seconds: 3600 }),
    block({ duration_seconds: 1800 }),
    block({ duration_seconds: 900, is_personal: true }),
    block({ duration_seconds: 600, ignored_at: '2026-10-05T10:00:00Z' }),
  ]
  expect(workSeconds(blocks)).toBe(5400)
  expect(workSeconds([])).toBe(0)
})

test('formatHours', () => {
  expect(formatHours(9000)).toBe('2h30')
  expect(formatHours(300)).toBe('0h05')
  expect(formatHours(0)).toBe('0h00')
  expect(formatHours(36000)).toBe('10h00')
})

test('ticketFromBranch takes the first key and none for absent or keyless branches', () => {
  expect(ticketFromBranch('feature/GOJ-1310-fix-thing')).toBe('GOJ-1310')
  expect(ticketFromBranch('AB-1-and-CD-2')).toBe('AB-1')
  expect(ticketFromBranch('main')).toBeUndefined()
  expect(ticketFromBranch('goj-1310')).toBeUndefined()
  expect(ticketFromBranch('')).toBeUndefined()
  expect(ticketFromBranch(undefined)).toBeUndefined()
})

test('reviewRequest maps each action to its daemon route and body', () => {
  expect(reviewRequest(7, { kind: 'personal', is_personal: true })).toEqual({
    path: '/blocks/7/personal',
    body: { is_personal: true },
  })
  expect(reviewRequest(7, { kind: 'ignore', ignored: false })).toEqual({
    path: '/blocks/7/ignore',
    body: { ignored: false },
  })
  expect(reviewRequest(7, { kind: 'ticket', jira_issue: null })).toEqual({
    path: '/blocks/7/ticket',
    body: { jira_issue: null },
  })
  expect(reviewRequest(7, { kind: 'description', description: 'did it' })).toEqual({
    path: '/blocks/7/description',
    body: { description: 'did it' },
  })
})

test('daemonGet resolves the parsed body from the base url plus the path', async () => {
  const fake = fakeEngine({ fetch: () => reply(200, '{"total_seconds":5}') })
  expect(await daemonGet(fake.engine, '/days/2026-10-05')).toEqual({
    ok: true,
    value: { total_seconds: 5 },
  })
  expect(fake.fetched[0].url).toBe('http://127.0.0.1:9323/days/2026-10-05')
})

test('daemonGet uses the body error field, else the status', async () => {
  const withError = fakeEngine({ fetch: () => reply(404, '{"error":"no such day"}') })
  expect(await daemonGet(withError.engine, '/x')).toEqual({ ok: false, error: 'no such day' })
  const withoutError = fakeEngine({ fetch: () => reply(404, 'not json') })
  expect(await daemonGet(withoutError.engine, '/x')).toEqual({ ok: false, error: 'HTTP 404' })
})

test('daemonGet resolves the fetch error message when fetch rejects', async () => {
  const fake = fakeEngine({ fetch: () => Promise.reject(new Error('connection refused')) })
  expect(await daemonGet(fake.engine, '/x')).toEqual({ ok: false, error: 'connection refused' })
})

test('daemonGet resolves timeout when the daemon does not answer in time', async () => {
  const fake = fakeEngine({ fetch: () => new Promise(() => {}) })
  const pending = daemonGet(fake.engine, '/x')
  fake.fireTimeout()
  expect(await pending).toEqual({ ok: false, error: 'timeout' })
})

test('daemonPost sends a JSON body with the POST method', async () => {
  const fake = fakeEngine({ fetch: () => reply(200, '{"id":7}') })
  expect(await daemonPost(fake.engine, '/blocks/7/ignore', { ignored: true })).toEqual({
    ok: true,
    value: { id: 7 },
  })
  expect(fake.fetched[0].init?.method).toBe('POST')
  expect(fake.fetched[0].init?.body).toBe('{"ignored":true}')
})

test('daemonToday reads today from /logged for the UTC day of the clock', async () => {
  const fake = fakeEngine({
    now: Date.parse('2026-10-05T23:30:00Z'),
    fetch: () => reply(200, '{"today":"2026-10-06"}'),
  })
  expect(await daemonToday(fake.engine)).toEqual({ ok: true, value: '2026-10-06' })
  expect(fake.fetched[0].url).toBe('http://127.0.0.1:9323/logged?from=2026-10-05&to=2026-10-05')
})

test('workContext: under ~/Desktop/Work is work and carries branch and ticket', async () => {
  const fake = fakeEngine({
    home: '/Users/me',
    git: { 'git branch --show-current': 'feature/GOJ-1310-x\n' },
  })
  expect(await workContext(fake.engine, '/Users/me/Desktop/Work/app')).toEqual({
    cwd: '/Users/me/Desktop/Work/app',
    isWork: true,
    branch: 'feature/GOJ-1310-x',
    ticket: 'GOJ-1310',
  })
})

test('workContext: an aproorg origin outside the work folder is work', async () => {
  const fake = fakeEngine({
    home: '/Users/me',
    git: {
      'git branch --show-current': 'main\n',
      'git remote get-url origin': 'git@github.com:aproorg/thing.git\n',
    },
  })
  const context = await workContext(fake.engine, '/Users/me/code/thing')
  expect(context.isWork).toBe(true)
  expect(context.ticket).toBeUndefined()
})

test('workContext: another org origin outside the work folder is not work', async () => {
  const fake = fakeEngine({
    home: '/Users/me',
    git: {
      'git branch --show-current': 'main\n',
      'git remote get-url origin': 'https://github.com/someone/aproorg-fan.git\n',
    },
  })
  expect((await workContext(fake.engine, '/Users/me/code/thing')).isWork).toBe(false)
})

test('workContext: no repo and outside the work folder is not work, no branch', async () => {
  const fake = fakeEngine({ home: '/Users/me' })
  expect(await workContext(fake.engine, '/tmp/scratch')).toEqual({
    cwd: '/tmp/scratch',
    isWork: false,
    branch: undefined,
    ticket: undefined,
  })
})

test('workContext: detached HEAD (empty branch) has no branch and no ticket', async () => {
  const fake = fakeEngine({ home: '/Users/me', git: { 'git branch --show-current': '\n' } })
  const context = await workContext(fake.engine, '/Users/me/Desktop/Work/app')
  expect(context.isWork).toBe(true)
  expect(context.branch).toBeUndefined()
  expect(context.ticket).toBeUndefined()
})

test('workContext: a git spawn failure leaves the path rule alone', async () => {
  const fake = fakeEngine({ home: '/Users/me' })
  fake.engine.process.run = () => Promise.reject(new Error('spawn git ENOENT'))
  const context = await workContext(fake.engine, '/Users/me/Desktop/Work/app')
  expect(context.isWork).toBe(true)
  expect(context.branch).toBeUndefined()
})
