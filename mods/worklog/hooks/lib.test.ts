import { test, expect } from 'claude-code/testing'
import {
  daemonGet,
  daemonPost,
  daemonToday,
  formatHours,
  lastWorkday,
  mondayOf,
  reviewRequest,
  ticketFromBranch,
  keyOfChoice,
  ticketChoices,
  workContext,
  workSeconds,
} from './lib'
import type { Block, Io, RecentTask } from './contract'

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
  io: Io
  fetched: Fetched[]
  fireTimeout: () => void
}

const fakeIo = (options: {
  fetch?: (url: string, init?: Fetched['init']) => Promise<{ status: number; ok: boolean; text: string }>
  git?: Record<string, string>
  home?: string
  now?: number
}): Fake => {
  const fetched: Fetched[] = []
  let timeoutFunction: (() => void) | undefined
  const io: Io = {
    fetch: (url, init) => {
      fetched.push({ url, init })
      return options.fetch?.(url, init) ?? new Promise(() => {})
    },
    run: async argv => {
      const stdout = options.git?.[argv.join(' ')]
      return { exitCode: stdout === undefined ? 1 : 0, stdout: stdout ?? '' }
    },
    after: (_milliseconds, callback) => {
      timeoutFunction = callback
      return { cancel: () => (timeoutFunction = undefined) }
    },
    now: async () => options.now ?? 0,
    home: options.home,
  }
  return { io, fetched, fireTimeout: () => timeoutFunction?.() }
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
  const fake = fakeIo({ fetch: () => reply(200, '{"total_seconds":5}') })
  expect(await daemonGet(fake.io, '/days/2026-10-05')).toEqual({
    ok: true,
    value: { total_seconds: 5 },
  })
  expect(fake.fetched[0].url).toBe('http://127.0.0.1:9323/days/2026-10-05')
})

test('daemonGet uses the body error field, else the status', async () => {
  const withError = fakeIo({ fetch: () => reply(404, '{"error":"no such day"}') })
  expect(await daemonGet(withError.io, '/x')).toEqual({ ok: false, error: 'no such day' })
  const withoutError = fakeIo({ fetch: () => reply(404, 'not json') })
  expect(await daemonGet(withoutError.io, '/x')).toEqual({ ok: false, error: 'HTTP 404' })
})

test('daemonGet resolves the fetch error message when fetch rejects', async () => {
  const fake = fakeIo({ fetch: () => Promise.reject(new Error('connection refused')) })
  expect(await daemonGet(fake.io, '/x')).toEqual({ ok: false, error: 'connection refused' })
})

test('daemonGet resolves timeout when the daemon does not answer in time', async () => {
  const fake = fakeIo({ fetch: () => new Promise(() => {}) })
  const pending = daemonGet(fake.io, '/x')
  fake.fireTimeout()
  expect(await pending).toEqual({ ok: false, error: 'timeout' })
})

test('daemonPost sends a JSON body with the POST method', async () => {
  const fake = fakeIo({ fetch: () => reply(200, '{"id":7}') })
  expect(await daemonPost(fake.io, '/blocks/7/ignore', { ignored: true })).toEqual({
    ok: true,
    value: { id: 7 },
  })
  expect(fake.fetched[0].init?.method).toBe('POST')
  expect(fake.fetched[0].init?.body).toBe('{"ignored":true}')
})

test('daemonToday reads today from /logged for the UTC day of the clock', async () => {
  const fake = fakeIo({
    now: Date.parse('2026-10-05T23:30:00Z'),
    fetch: () => reply(200, '{"today":"2026-10-06"}'),
  })
  expect(await daemonToday(fake.io)).toEqual({ ok: true, value: '2026-10-06' })
  expect(fake.fetched[0].url).toBe('http://127.0.0.1:9323/logged?from=2026-10-05&to=2026-10-05')
})

test('workContext: under ~/Desktop/Work is work and carries branch and ticket', async () => {
  const fake = fakeIo({
    home: '/Users/me',
    git: { 'git branch --show-current': 'feature/GOJ-1310-x\n' },
  })
  expect(await workContext(fake.io, '/Users/me/Desktop/Work/app')).toEqual({
    cwd: '/Users/me/Desktop/Work/app',
    isWork: true,
    branch: 'feature/GOJ-1310-x',
    ticket: 'GOJ-1310',
  })
})

test('workContext: an aproorg origin outside the work folder is work', async () => {
  const fake = fakeIo({
    home: '/Users/me',
    git: {
      'git branch --show-current': 'main\n',
      'git remote get-url origin': 'git@github.com:aproorg/thing.git\n',
    },
  })
  const context = await workContext(fake.io, '/Users/me/code/thing')
  expect(context.isWork).toBe(true)
  expect(context.ticket).toBeUndefined()
})

test('workContext: another org origin outside the work folder is not work', async () => {
  const fake = fakeIo({
    home: '/Users/me',
    git: {
      'git branch --show-current': 'main\n',
      'git remote get-url origin': 'https://github.com/someone/aproorg-fan.git\n',
    },
  })
  expect((await workContext(fake.io, '/Users/me/code/thing')).isWork).toBe(false)
})

test('workContext: no repo and outside the work folder is not work, no branch', async () => {
  const fake = fakeIo({ home: '/Users/me' })
  expect(await workContext(fake.io, '/tmp/scratch')).toEqual({
    cwd: '/tmp/scratch',
    isWork: false,
    branch: undefined,
    ticket: undefined,
  })
})

test('workContext: detached HEAD (empty branch) has no branch and no ticket', async () => {
  const fake = fakeIo({ home: '/Users/me', git: { 'git branch --show-current': '\n' } })
  const context = await workContext(fake.io, '/Users/me/Desktop/Work/app')
  expect(context.isWork).toBe(true)
  expect(context.branch).toBeUndefined()
  expect(context.ticket).toBeUndefined()
})

test('workContext: a git spawn failure leaves the path rule alone', async () => {
  const fake = fakeIo({ home: '/Users/me' })
  fake.io.run = () => Promise.reject(new Error('spawn git ENOENT'))
  const context = await workContext(fake.io, '/Users/me/Desktop/Work/app')
  expect(context.isWork).toBe(true)
  expect(context.branch).toBeUndefined()
})

const task = (key: string, last: string | null, assigned = true): RecentTask => ({
  key,
  summary: `sum ${key}`,
  assigned,
  last_worked_day: last,
})

test('ticketChoices: no branch key takes the 2 newest, then Create and Skip', () => {
  const tasks = [task('AB-1', '2026-10-01'), task('AB-3', '2026-10-03'), task('AB-2', '2026-10-02')]
  // wrong version: unsorted slice would give AB-1, AB-3; taking 3 would exceed 4
  expect(ticketChoices(undefined, tasks)).toEqual([
    'AB-3 sum AB-3',
    'AB-2 sum AB-2',
    'Create a new ticket',
    'Skip',
  ])
})

test('ticketChoices: a branch key leads, with 1 recent task (4 total)', () => {
  const tasks = [task('AB-3', '2026-10-03'), task('AB-2', '2026-10-02')]
  // wrong version: still taking 2 recents gives 5 choices
  expect(ticketChoices('GENAI-42', tasks)).toEqual([
    'GENAI-42',
    'AB-3 sum AB-3',
    'Create a new ticket',
    'Skip',
  ])
})

test('ticketChoices: the branch key is deduped out of recents before slicing', () => {
  const tasks = [task('GENAI-42', '2026-10-04'), task('AB-3', '2026-10-03')]
  // wrong version: slice then filter drops the only recent
  expect(ticketChoices('GENAI-42', tasks)).toEqual([
    'GENAI-42',
    'AB-3 sum AB-3',
    'Create a new ticket',
    'Skip',
  ])
})

test('ticketChoices: null last_worked_day sorts last', () => {
  const tasks = [task('AB-1', null), task('AB-2', '2026-10-02')]
  // wrong version: null compared as smallest-first or string sort placing it first
  expect(ticketChoices(undefined, tasks).slice(0, 2)).toEqual(['AB-2 sum AB-2', 'AB-1 sum AB-1'])
})

test('ticketChoices: unassigned tasks are excluded', () => {
  // wrong version: no assigned filter
  expect(ticketChoices(undefined, [task('AB-1', '2026-10-01', false)])).toEqual([
    'Create a new ticket',
    'Skip',
  ])
})

test('ticketChoices: empty list and short list give fewer choices', () => {
  expect(ticketChoices('GENAI-42', [])).toEqual(['GENAI-42', 'Create a new ticket', 'Skip'])
  expect(ticketChoices(undefined, [task('AB-1', null)])).toEqual([
    'AB-1 sum AB-1',
    'Create a new ticket',
    'Skip',
  ])
})

test('ticketChoices: does not mutate the input list', () => {
  const tasks = [task('AB-1', '2026-10-01'), task('AB-2', '2026-10-02')]
  ticketChoices(undefined, tasks)
  expect(tasks.map((t) => t.key)).toEqual(['AB-1', 'AB-2'])
})

test('keyOfChoice: leading key of a task label or bare key', () => {
  expect(keyOfChoice('AB-3 sum AB-3')).toBe('AB-3')
  expect(keyOfChoice('GENAI-42')).toBe('GENAI-42')
})

test('keyOfChoice: Create, Skip and free text are not keys', () => {
  expect(keyOfChoice('Create a new ticket')).toBeUndefined()
  expect(keyOfChoice('Skip')).toBeUndefined()
  // wrong version: unanchored match finds a key mid-text
  expect(keyOfChoice('see GENAI-42 please')).toBeUndefined()
  expect(keyOfChoice('')).toBeUndefined()
})

test('ticketChoices: a likely key leads, labelled when known, then the branch, then fills to 2', () => {
  const tasks = [task('AB-3', '2026-10-03'), task('AB-2', '2026-10-02')]
  expect(ticketChoices('GENAI-42', tasks, 'AB-2')).toEqual([
    'AB-2 sum AB-2',
    'GENAI-42',
    'Create a new ticket',
    'Skip',
  ])
  expect(ticketChoices(undefined, tasks, 'AB-2')).toEqual([
    'AB-2 sum AB-2',
    'AB-3 sum AB-3',
    'Create a new ticket',
    'Skip',
  ])
})

test('ticketChoices: a likely key equal to the branch shows once; unknown likely is a bare key', () => {
  expect(ticketChoices('GENAI-42', [], 'GENAI-42')).toEqual(['GENAI-42', 'Create a new ticket', 'Skip'])
  expect(ticketChoices(undefined, [], 'ZZ-1')).toEqual(['ZZ-1', 'Create a new ticket', 'Skip'])
})
