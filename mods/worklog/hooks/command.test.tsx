import { test, expect, mock } from 'claude-code/testing'
import type { On } from 'claude-code'
import type { Block, CloseoutDay } from './contract'

const SESSION = { surface: 'terminal', isInteractive: true, cwd: '/Users/me/Desktop/Work/app' } as const
const TODAY = '2026-10-07'
const PANE = {
  component: 'Pane',
  surface: 'terminal',
  requestId: 'worklog-review',
  viewport: { columns: 120, rows: 40, isFullscreen: true },
  props: {
    title: 'worklog review',
    isFocused: true,
    bodyColumns: 100,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 36 },
    view: {},
  },
} as const

const command = (args: string) => ({
  command: 'wl',
  args,
  origin: { kind: 'composer' as const },
  presentation: { isFullscreen: true, columns: 120 },
})

const block = (overrides: Partial<Block>) => ({
  id: 1,
  day: TODAY,
  jira_issue: null,
  started_at: '2026-10-07T08:00:00Z',
  ended_at: '2026-10-07T09:00:00Z',
  duration_seconds: 3600,
  description: null,
  is_personal: false,
  ignored_at: null,
  tempo_worklog_id: null,
  exported_at: null,
  project: null,
  ...overrides,
})

const closeoutDay = (day: string, loggedSeconds: number): CloseoutDay => ({
  day,
  logged_seconds: loggedSeconds,
  synced_seconds: 0,
  tempo_seconds: 0,
  required_seconds: 8 * 3600,
  pending_lines: 0,
})

type Options = {
  blocks?: ReturnType<typeof block>[]
  closeoutDays?: CloseoutDay[]
  refusal?: string
  isDown?: boolean
}

type Posted = { url: string; body: unknown }

function world(on: On, options: Options = {}) {
  const requested: string[] = []
  const posted: Posted[] = []
  const opened: { id: string; focus: boolean; closeOnEscape: boolean }[] = []
  const registered: { name: string; argumentHint: string | undefined }[] = []
  const answer = (status: number, body: unknown) => ({
    value: { status, ok: status < 300, headers: {}, text: JSON.stringify(body) },
  })

  on('session.start', ($, event) => ({ cwd: event.cwd }))
  on('env.get', () => ({ value: '/Users/me' }))
  on('command.register', ($, event) => {
    registered.push({ name: event.name, argumentHint: event.argumentHint })
    return { value: { command: event.name } }
  })
  on('ui.open', ($, event) => {
    opened.push({ id: event.id, focus: event.focus === true, closeOnEscape: event.closeOnEscape === true })
    return { value: { isPlaced: true } } as never
  })
  on('ui.status', () => ({ value: undefined }))
  on('ui.toast', () => ({ value: undefined }))
  on('store.get', () => ({ value: undefined }))
  on('store.set', () => ({ value: undefined }))
  on('process.run', () => ({ value: { exitCode: 1, stdout: '' } }))
  on('http.fetch', ($, event) => {
    if (options.isDown) return { deny: 'connection refused' }
    if (event.init?.method === 'POST') {
      posted.push({ url: event.url, body: JSON.parse(event.init.body ?? 'null') })
      return options.refusal === undefined
        ? answer(200, { ok: true })
        : answer(400, { error: options.refusal })
    }
    requested.push(event.url)
    if (event.url.includes('/logged')) return answer(200, { today: TODAY })
    if (event.url.includes('/days/')) {
      return answer(200, { day: TODAY, total_seconds: 0, blocks: options.blocks ?? [] })
    }
    return answer(200, { days: options.closeoutDays ?? [] })
  })
  return { requested, posted, opened, registered }
}

type Node = string | { type?: string; props?: Record<string, unknown>; children?: Node[] }

function elementsOf(node: Node | null | undefined, type: string): Extract<Node, object>[] {
  if (node === null || node === undefined || typeof node === 'string') return []
  const own = node.type === type ? [node] : []
  return [...own, ...(node.children ?? []).flatMap(child => elementsOf(child, type))]
}

function textOf(node: Node | null | undefined): string {
  if (node === null || node === undefined) return ''
  if (typeof node === 'string') return node
  return (node.children ?? []).map(textOf).join('')
}

const hours = [
  block({ id: 1, duration_seconds: 5400 }),
  block({ id: 2, duration_seconds: 3600, is_personal: true }),
  block({ id: 3, duration_seconds: 1800, ignored_at: '2026-10-07T10:00:00Z' }),
]

test('session start registers /wl with its argument hint', async ($, on) => {
  const seen = world(on)
  mock.clock(on)
  await $.session.start(SESSION)
  expect(seen.registered).toEqual([{ name: 'wl', argumentHint: 'today|week|review' }])
})

test('/wl today prints the hours of the blocks that are neither personal nor ignored', async ($, on) => {
  const seen = world(on, { blocks: hours })
  mock.clock(on)
  await $.session.start(SESSION)
  const result = await $.command.run(command('today'))
  expect(result.text).toBe('worklog today: 1h30')
  expect(seen.requested).toContain(`http://127.0.0.1:9323/days/${TODAY}`)
})

test('/wl week prints the sum of the logged seconds of the week of today', async ($, on) => {
  const seen = world(on, {
    closeoutDays: [closeoutDay('2026-10-05', 8 * 3600), closeoutDay('2026-10-06', 1800)],
  })
  mock.clock(on)
  await $.session.start(SESSION)
  const result = await $.command.run(command('week'))
  expect(result.text).toBe('worklog week: 8h30')
  expect(seen.requested).toContain('http://127.0.0.1:9323/weeks/2026-10-05/closeout')
})

test('any other argument, or none, prints the usage', async ($, on) => {
  world(on)
  mock.clock(on)
  await $.session.start(SESSION)
  expect((await $.command.run(command('year'))).text).toBe('usage: /wl today|week|review')
  expect((await $.command.run(command(''))).text).toBe('usage: /wl today|week|review')
})

test('/wl today says so when the daemon is down', async ($, on) => {
  world(on, { isDown: true })
  mock.clock(on)
  await $.session.start(SESSION)
  const result = await $.command.run(command('today'))
  expect(result.text).toContain('worklog:')
  expect(result.text).toContain('connection refused')
})

test('/wl review opens a focused pane that closes on Escape', async ($, on) => {
  const seen = world(on, { blocks: hours })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  expect(seen.opened).toEqual([{ id: 'worklog-review', focus: true, closeOnEscape: true }])
})

test('the pane lists today non-ignored blocks, personal ones included, with the four hotkeys', async ($, on) => {
  world(on, { blocks: hours })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  const tree = (await $.ui.render(PANE)) as Node
  const options = elementsOf(tree, 'Select').flatMap(select => select.props?.options as { value: string }[])
  expect(options.map(option => option.value)).toEqual(['1', '2'])
  const buttons = elementsOf(tree, 'Button').map(button => [button.props?.key, button.props?.hotkey])
  expect(buttons).toEqual([
    ['personal', 'p'],
    ['ignore', 'i'],
    ['ticket', 't'],
    ['description', 'd'],
  ])
})

test('p and i post the toggled value for the selected block and reload the list', async ($, on) => {
  const seen = world(on, { blocks: hours })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  await $.ui.render(PANE)
  const dayReads = () => seen.requested.filter(url => url.includes('/days/')).length
  const before = dayReads()
  await $.ui.select({ plugin: 'worklog', key: 'block', value: '2' })
  await $.ui.press({ plugin: 'worklog', key: 'personal' })
  await $.ui.render(PANE)
  await $.ui.press({ plugin: 'worklog', key: 'ignore' })
  await $.ui.render(PANE)
  expect(seen.posted).toEqual([
    { url: 'http://127.0.0.1:9323/blocks/2/personal', body: { is_personal: false } },
    { url: 'http://127.0.0.1:9323/blocks/2/ignore', body: { ignored: true } },
  ])
  expect(dayReads() - before).toBe(2)
})

test('t sends the typed ticket on Enter, and null for empty text', async ($, on) => {
  const seen = world(on, { blocks: [block({ id: 5, jira_issue: 'GOJ-1' })] })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  await $.ui.render(PANE)
  await $.ui.press({ plugin: 'worklog', key: 'ticket' })
  const editing = (await $.ui.render(PANE)) as Node
  expect(elementsOf(editing, 'Input')[0].props?.value).toBe('GOJ-1')
  await $.ui.input({ plugin: 'worklog', key: 'edit', text: 'GOJ-2' })
  await $.ui.press({ plugin: 'worklog', key: 'ticket' })
  await $.ui.input({ plugin: 'worklog', key: 'edit', text: '' })
  expect(seen.posted.map(post => [post.url, post.body])).toEqual([
    ['http://127.0.0.1:9323/blocks/5/ticket', { jira_issue: 'GOJ-2' }],
    ['http://127.0.0.1:9323/blocks/5/ticket', { jira_issue: null }],
  ])
})

test('d sends the typed description, and cancel sends nothing', async ($, on) => {
  const seen = world(on, { blocks: [block({ id: 5, description: 'old' })] })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  await $.ui.render(PANE)
  await $.ui.press({ plugin: 'worklog', key: 'description' })
  expect(elementsOf((await $.ui.render(PANE)) as Node, 'Input')[0].props?.value).toBe('old')
  await $.ui.press({ plugin: 'worklog', key: 'cancel' })
  expect(elementsOf((await $.ui.render(PANE)) as Node, 'Input')).toEqual([])
  await $.ui.press({ plugin: 'worklog', key: 'description' })
  await $.ui.render(PANE)
  await $.ui.input({ plugin: 'worklog', key: 'edit', text: 'new' })
  expect(seen.posted).toEqual([
    { url: 'http://127.0.0.1:9323/blocks/5/description', body: { description: 'new' } },
  ])
})

test('a refused action shows the daemon error text in the pane', async ($, on) => {
  world(on, { blocks: hours, refusal: 'block already synced' })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  await $.ui.render(PANE)
  await $.ui.press({ plugin: 'worklog', key: 'personal' })
  expect(textOf((await $.ui.render(PANE)) as Node)).toContain('block already synced')
})

test('the Select labels are aligned columns: time range, hours, ticket, personal marker, description', async ($, on) => {
  world(on, {
    blocks: [
      block({
        id: 1,
        started_at: '2026-10-07T11:44:00Z',
        ended_at: '2026-10-07T12:19:00Z',
        duration_seconds: 35 * 60,
        is_personal: true,
      }),
      block({ id: 2, jira_issue: 'GOJ-1', description: 'x'.repeat(50) }),
    ],
  })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  const tree = (await $.ui.render(PANE)) as Node
  const options = elementsOf(tree, 'Select').flatMap(select => select.props?.options as { label: string }[])
  expect(options[0].label).toBe(`11:44–12:19   0h35  —${' '.repeat(13)}personal`)
  expect(options[1].label).toBe(`08:00–09:00   1h00  GOJ-1${' '.repeat(9)}${'x'.repeat(39)}…`)
})

test('the personal button reads work for a personal block and personal otherwise', async ($, on) => {
  world(on, { blocks: hours })
  mock.clock(on)
  await $.session.start(SESSION)
  await $.command.run(command('review'))
  const label = async () =>
    elementsOf((await $.ui.render(PANE)) as Node, 'Button').find(b => b.props?.key === 'personal')?.props?.label
  expect(await label()).toBe('personal')
  await $.ui.select({ plugin: 'worklog', key: 'block', value: '2' })
  expect(await label()).toBe('work')
})
