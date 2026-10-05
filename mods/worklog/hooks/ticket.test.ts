import { test, expect, mock } from 'claude-code/testing'
import type { On } from 'claude-code'

const WORK_CWD = '/home/owner/Desktop/Work/api'

type Seen = {
  asks: { question: string; header: string; options: string[] }[]
  runs: string[][]
  logs: string[]
  stored: Record<string, unknown>
}

type World = {
  tasks?: unknown
  isDown?: boolean
  slowTasks?: boolean
  answer?: string
  gate?: Promise<void>
  dismiss?: boolean
  recordFails?: boolean
  stored?: Record<string, unknown>
  id?: string
}

const seat = (on: On, branch: { current: string | undefined }, world: World = {}) => {
  const toasts: string[] = []
  const seen: Seen = { asks: [], runs: [], logs: [], stored: { ...world.stored } }
  on('http.fetch', async (_$, e) => {
    if (world.isDown || !e.url.endsWith('/tasks')) return { deny: 'connection refused' }
    if (world.slowTasks) await new Promise(() => {})
    const body = { tasks: world.tasks ?? [] }
    return { value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } }
  })
  on('store.get', (_$, e) => ({ value: seen.stored[e.key] }))
  on('store.set', (_$, e) => {
    seen.stored[e.key] = e.value
    return { value: undefined }
  })
  on('ui.log', (_$, e) => {
    seen.logs.push(e.text)
    return { value: undefined }
  })
  on('session.id', () => ({ value: world.id ?? SESSION_ID }))
  on('session.end', () => ({ sessionId: world.id ?? SESSION_ID }))
  on('tool.call', async (_$, e) => {
    const q = e.questions[0]
    seen.asks.push({ question: q.question, header: q.header, options: q.options.map(o => o.label) })
    await world.gate
    if (world.dismiss) return { deny: 'dismissed' }
    return { result: { answers: { [q.question]: world.answer ?? 'Skip' } } }
  })
  on('session.start', (_$, e) => ({ cwd: e.cwd }))
  on('prompt.context', (_$, e) => ({ blocks: e.blocks }))
  const clock = mock.clock(on)
  on('attribution.text', (_$, e) => ({ text: e.text }))
  on('turn.complete', (_$, e) => ({ text: e.answer }))
  on('env.get', (_$, e) => ({ value: e.name === 'HOME' ? '/home/owner' : undefined }))
  on('ui.toast', (_$, e) => {
    toasts.push(e.text)
    return { value: undefined }
  })
  on('process.run', (_$, e) => {
    if (e.argv[0] === 'worklog') {
      seen.runs.push(e.argv)
      return world.recordFails
        ? { value: { exitCode: 1, stdout: '', stderr: 'no daemon' } }
        : { value: { exitCode: 0, stdout: '', stderr: '' } }
    }
    const isBranch = e.argv.join(' ') === 'git branch --show-current'
    return isBranch && branch.current !== undefined
      ? { value: { exitCode: 0, stdout: `${branch.current}\n`, stderr: '' } }
      : { value: { exitCode: 1, stdout: '', stderr: '' } }
  })
  return { toasts, clock, seen }
}

const OTHER = { blocks: [{ name: 'other', text: 'kept' }] }

const start = (cwd: string) => ({ surface: 'terminal' as const, isInteractive: true, cwd })

test('session start in a work folder on a ticket branch toasts the ticket', async ($, on) => {
  const { toasts } = seat(on, { current: 'GENAI-9-fix-thing' })
  await $.session.start(start(WORK_CWD))
  expect(toasts).toEqual(['worklog: GENAI-9'])
})

test('no toast, context or trailer on a branch without a ticket', async ($, on) => {
  const { toasts } = seat(on, { current: 'main' })
  await $.session.start(start(WORK_CWD))
  expect(toasts).toEqual([])
  expect(await $.prompt.context(OTHER)).toEqual(OTHER)
  expect(await $.attribution.text({ kind: 'commit', text: 'msg' })).toEqual({ text: 'msg' })
})

test('no toast, context or trailer outside a work folder', async ($, on) => {
  const { toasts } = seat(on, { current: 'GENAI-9-x' })
  await $.session.start(start('/tmp/play'))
  expect(toasts).toEqual([])
  expect(await $.prompt.context(OTHER)).toEqual(OTHER)
  expect(await $.attribution.text({ kind: 'pr', text: 'msg' })).toEqual({ text: 'msg' })
})

test('the context block names the ticket and keeps the other blocks', async ($, on) => {
  seat(on, { current: 'GENAI-9-fix-thing' })
  await $.session.start(start(WORK_CWD))
  const expected = {
    blocks: [
      { name: 'other', text: 'kept' },
      { name: 'worklog', text: 'Current branch ticket: GENAI-9' },
    ],
  }
  expect(await $.prompt.context(OTHER)).toEqual(expected)
  expect(await $.prompt.context(OTHER)).toEqual(expected)
})

test('commit and PR attribution end with the ticket trailer', async ($, on) => {
  seat(on, { current: 'GENAI-9-fix-thing' })
  await $.session.start(start(WORK_CWD))
  expect(await $.attribution.text({ kind: 'commit', text: 'msg' })).toEqual({
    text: 'msg\n\nTicket: GENAI-9',
  })
  expect(await $.attribution.text({ kind: 'pr', text: 'body' })).toEqual({
    text: 'body\n\nTicket: GENAI-9',
  })
})

test('a branch change is picked up after the turn completes', async ($, on) => {
  const branch = { current: 'GENAI-9-a' as string | undefined }
  const { clock } = seat(on, branch)
  await $.session.start(start(WORK_CWD))
  branch.current = 'GENAI-10-b'
  await $.turn.complete({ turnId: 't1', answer: 'done' })
  await clock.advance(50)
  expect(await $.attribution.text({ kind: 'commit', text: 'msg' })).toEqual({
    text: 'msg\n\nTicket: GENAI-10',
  })
})

const SESSION_ID = 'sess-1'
const QUESTION ='Which Jira ticket is this session for?'
const TASKS = [
  { key: 'GENAI-1', summary: 'Older', assigned: true, last_worked_day: '2026-10-01' },
  { key: 'GENAI-2', summary: 'Newer', assigned: true, last_worked_day: '2026-10-04' },
  { key: 'GENAI-3', summary: 'Never', assigned: true, last_worked_day: null },
]
const NO_RECENTS = ['GENAI-9', 'Create a new ticket', 'Skip']

const asked = (world: World, cwd = WORK_CWD, branch: string | undefined = 'GENAI-9-fix') =>
  async ($: Parameters<Parameters<typeof test>[1]>[0], on: On) => {
    const { clock, seen, toasts } = seat(on, { current: branch }, world)
    await $.session.start(start(cwd))
    await clock.advance(10)
    return { seen, toasts, clock }
  }

test('a fresh work session asks one ticket question: branch ticket, recent, create, skip', async ($, on) => {
  const { seen } = await asked({ tasks: TASKS })($, on)
  expect(seen.asks).toEqual([
    {
      question: QUESTION,
      header: 'Ticket',
      options: ['GENAI-9', 'GENAI-2 Newer', 'Create a new ticket', 'Skip'],
    },
  ])
})

test('no branch ticket offers two recent tickets, newest day first', async ($, on) => {
  const { seen } = await asked({ tasks: TASKS }, WORK_CWD, 'main')($, on)
  expect(seen.asks[0].options).toEqual(['GENAI-2 Newer', 'GENAI-1 Older', 'Create a new ticket', 'Skip'])
})

test('a non-interactive start asks nothing', async ($, on) => {
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' })
  await $.session.start({ surface: 'terminal', isInteractive: false, cwd: WORK_CWD })
  await clock.advance(10)
  expect(seen.asks).toEqual([])
})

test('outside a work folder asks nothing', async ($, on) => {
  const { seen } = await asked({}, '/tmp/play')($, on)
  expect(seen.asks).toEqual([])
})

test('a stored answer for the session id asks nothing', async ($, on) => {
  const id = SESSION_ID
  const { seen } = await asked({ stored: { [`ticket-asked:${id}`]: true } })($, on)
  expect(seen.asks).toEqual([])
})

test('the answer is stored under the session key so a second start asks nothing', async ($, on) => {
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' })
  await $.session.start(start(WORK_CWD))
  await clock.advance(10)
  const id = SESSION_ID
  expect(seen.stored[`ticket-asked:${id}`]).toBeTruthy()
  await $.session.start(start(WORK_CWD))
  await clock.advance(10)
  expect(seen.asks.length).toBe(1)
})

test('picking the branch ticket records it for the session and toasts', async ($, on) => {
  const { seen, toasts } = await asked({ answer: 'GENAI-9' })($, on)
  const id = SESSION_ID
  expect(seen.runs).toEqual([['worklog', 'ticket', 'use', 'GENAI-9', '--session', id]])
  expect(toasts).toEqual(['worklog: GENAI-9', 'worklog: GENAI-9'])
})

test('picking a recent ticket label records its key, not the label', async ($, on) => {
  const { seen } = await asked({ tasks: TASKS, answer: 'GENAI-2 Newer' })($, on)
  expect(seen.runs.map(argv => argv[3])).toEqual(['GENAI-2'])
})

test('typed text that is exactly a key records it', async ($, on) => {
  const { seen } = await asked({ answer: 'ABC-123' })($, on)
  expect(seen.runs.map(argv => argv[3])).toEqual(['ABC-123'])
})

test('typed text with a key plus words records nothing', async ($, on) => {
  const { seen } = await asked({ answer: 'ABC-123 and more' })($, on)
  expect(seen.runs).toEqual([])
})

test('typed text that is not a key records nothing', async ($, on) => {
  const { seen } = await asked({ answer: 'the login bug' })($, on)
  expect(seen.runs).toEqual([])
})

test('Skip records nothing', async ($, on) => {
  const { seen } = await asked({ answer: 'Skip' })($, on)
  expect(seen.runs).toEqual([])
})

test('Create a new ticket records nothing', async ($, on) => {
  const { seen } = await asked({ answer: 'Create a new ticket' })($, on)
  expect(seen.runs).toEqual([])
})

test('Skip stores the answer so the session is not asked again', async ($, on) => {
  const { seen } = await asked({ answer: 'Skip' })($, on)
  const id = SESSION_ID
  expect(seen.stored[`ticket-asked:${id}`]).toBeTruthy()
})

test('a dismissed dialog records nothing, logs the manual command and is not asked again', async ($, on) => {
  const { seen } = await asked({ dismiss: true })($, on)
  const id = SESSION_ID
  expect(seen.runs).toEqual([])
  expect(seen.logs).toEqual([
    `worklog: no ticket question shown — run worklog ticket use KEY --session ${id}`,
  ])
  expect(seen.stored[`ticket-asked:${id}`]).toBeTruthy()
})

test('a failed record logs the key and stderr', async ($, on) => {
  const { seen } = await asked({ answer: 'GENAI-9', recordFails: true })($, on)
  expect(seen.logs).toEqual(['worklog: could not record GENAI-9 — no daemon'])
})

test('a failed record does not toast the key a second time', async ($, on) => {
  const { toasts } = await asked({ answer: 'GENAI-9', recordFails: true })($, on)
  expect(toasts).toEqual(['worklog: GENAI-9'])
})

test('daemon down still offers branch ticket, create and skip', async ($, on) => {
  const { seen } = await asked({ isDown: true })($, on)
  expect(seen.asks[0].options).toEqual(NO_RECENTS)
})

test('a daemon answer with no task list is treated as no recents', async ($, on) => {
  const { seen } = await asked({ tasks: 'nope' })($, on)
  expect(seen.asks[0].options).toEqual(NO_RECENTS)
})

test('the tasks lookup is capped at 1500 ms: not asked at 1499, asked without recents at 1500', async ($, on) => {
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' }, { slowTasks: true, tasks: TASKS })
  await $.session.start(start(WORK_CWD))
  await clock.advance(1499)
  expect(seen.asks).toEqual([])
  await clock.advance(1)
  expect(seen.asks[0].options).toEqual(NO_RECENTS)
})

test('long labels are cut to 60 characters ending in an ellipsis; 60 stays whole', async ($, on) => {
  const exact = 'x'.repeat(60 - 'GENAI-2 '.length)
  const tasks = [
    { key: 'GENAI-2', summary: exact, assigned: true, last_worked_day: '2026-10-04' },
    { key: 'GENAI-1', summary: `${exact}y`, assigned: true, last_worked_day: '2026-10-03' },
  ]
  const { seen } = await asked({ tasks }, WORK_CWD, 'main')($, on)
  const [first, second] = seen.asks[0].options
  expect(first).toBe(`GENAI-2 ${exact}`)
  expect(second).toBe(`${`GENAI-1 ${exact}y`.slice(0, 59)}…`)
  expect(second.length).toBe(60)
})

test('a cut label still records its key', async ($, on) => {
  const long = 'z'.repeat(80)
  const tasks = [{ key: 'GENAI-2', summary: long, assigned: true, last_worked_day: '2026-10-04' }]
  const label = `${`GENAI-2 ${long}`.slice(0, 59)}…`
  const { seen } = await asked({ tasks, answer: label }, WORK_CWD, 'main')($, on)
  expect(seen.runs.map(argv => argv[3])).toEqual(['GENAI-2'])
})

test('session start returns before the Owner answers', async ($, on) => {
  let release = () => {}
  const gate = new Promise<void>(resolve => {
    release = resolve
  })
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' }, { gate, answer: 'GENAI-9' })
  await $.session.start(start(WORK_CWD))
  await clock.advance(10)
  expect(seen.asks.length).toBe(1)
  expect(seen.runs).toEqual([])
  release()
  await clock.advance(10)
  expect(seen.runs.length).toBe(1)
})

const CREATE = (id: string) =>
  `Instruction "create a ticket": the Owner wants a new Jira ticket for this session. Once the first request makes the task clear, create it (worklog skill, Jira recipe, one confirm), then run worklog ticket use <KEY> --session ${id}.`
const FIND = (text: string, id: string) =>
  `Instruction "find the ticket for: ${text}": run worklog ticket find, confirm the match with the Owner, then run worklog ticket use <KEY> --session ${id}.`
const BRANCH_BLOCK = { name: 'worklog', text: 'Current branch ticket: GENAI-9' }
const blocksOf = async ($: Parameters<Parameters<typeof test>[1]>[0]) => (await $.prompt.context(OTHER)).blocks

test('Create hands off on every prompt, not just the first (create a ticket)', async ($, on) => {
  await asked({ answer: 'Create a new ticket' })($, on)
  const expected = [...OTHER.blocks, { name: 'worklog', text: `${BRANCH_BLOCK.text}\n${CREATE(SESSION_ID)}` }]
  expect(await blocksOf($)).toEqual(expected)
  expect(await blocksOf($)).toEqual(expected)
})

test('Create hands off even with no branch ticket (hand-off hidden behind the branch guard)', async ($, on) => {
  await asked({ answer: 'Create a new ticket' }, WORK_CWD, 'main')($, on)
  expect(await blocksOf($)).toEqual([...OTHER.blocks, { name: 'worklog', text: CREATE(SESSION_ID) }])
})

test('Skip adds no hand-off', async ($, on) => {
  await asked({ answer: 'Skip' })($, on)
  expect(await blocksOf($)).toEqual([...OTHER.blocks, BRANCH_BLOCK])
})

test('non-key Other text hands off find with that text', async ($, on) => {
  await asked({ answer: 'the login bug' })($, on)
  expect(await blocksOf($)).toEqual([...OTHER.blocks, { name: 'worklog', text: `${BRANCH_BLOCK.text}\n${FIND('the login bug', SESSION_ID)}` }])
})

test('a key plus words under Other is find text, not a recorded key', async ($, on) => {
  await asked({ answer: 'ABC-123 and more' })($, on)
  expect((await blocksOf($)).at(-1)).toEqual({ name: 'worklog', text: `${BRANCH_BLOCK.text}\n${FIND('ABC-123 and more', SESSION_ID)}` })
})

test('a recorded key replaces the branch ticket in context and attribution, with no hand-off', async ($, on) => {
  await asked({ answer: 'ABC-123' })($, on)
  expect(await blocksOf($)).toEqual([...OTHER.blocks, { name: 'worklog', text: 'Session ticket: ABC-123' }])
  expect(await $.attribution.text({ kind: 'commit', text: 'msg' })).toEqual({ text: 'msg\n\nTicket: ABC-123' })
  expect(await $.attribution.text({ kind: 'pr', text: 'msg' })).toEqual({ text: 'msg\n\nTicket: ABC-123' })
})

test('the recorded key survives the post-turn branch refresh', async ($, on) => {
  const { clock } = await asked({ answer: 'ABC-123' })($, on)
  await $.turn.complete({ turnId: 't1', answer: 'done' })
  await clock.advance(50)
  expect(await $.attribution.text({ kind: 'commit', text: 'msg' })).toEqual({ text: 'msg\n\nTicket: ABC-123' })
})

test('a failed record leaves the branch ticket in force', async ($, on) => {
  await asked({ answer: 'ABC-123', recordFails: true })($, on)
  expect(await blocksOf($)).toEqual([...OTHER.blocks, BRANCH_BLOCK])
})

test('clear asks once for the new session id', async ($, on) => {
  const world: World = { answer: 'Skip' }
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' }, world)
  await $.session.start(start(WORK_CWD))
  await clock.advance(10)
  world.id = 'sess-2'
  await $.session.end({ reason: 'clear' })
  await clock.advance(10)
  expect(seen.asks.length).toBe(2)
  expect(seen.stored['ticket-asked:sess-2']).toBeTruthy()
})

test('clear drops the old recorded key and hand-off, and records the new answer under the new id', async ($, on) => {
  const world: World = { answer: 'Create a new ticket' }
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' }, world)
  await $.session.start(start(WORK_CWD))
  await clock.advance(10)
  world.id = 'sess-2'
  world.answer = 'ABC-123'
  await $.session.end({ reason: 'clear' })
  await clock.advance(10)
  expect(seen.runs).toEqual([['worklog', 'ticket', 'use', 'ABC-123', '--session', 'sess-2']])
  expect(await blocksOf($)).toEqual([...OTHER.blocks, { name: 'worklog', text: 'Session ticket: ABC-123' }])
})

test('a session end for another reason asks nothing', async ($, on) => {
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' }, { stored: { [`ticket-asked:${SESSION_ID}`]: true } })
  await $.session.start(start(WORK_CWD))
  await $.session.end({ reason: 'logout' })
  await clock.advance(10)
  expect(seen.asks).toEqual([])
})

test('clear outside a work folder asks nothing', async ($, on) => {
  const { clock, seen } = seat(on, { current: 'GENAI-9-x' })
  await $.session.start(start('/tmp/play'))
  await $.session.end({ reason: 'clear' })
  await clock.advance(10)
  expect(seen.asks).toEqual([])
})
