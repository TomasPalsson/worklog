import { atom, read, update } from 'claude-code'
import type { EngineInterface, On, RenderInput } from 'claude-code'
import { REVIEW_PANE_ID } from './contract'
import type { DaySummary, Io, ReviewAction, WeekCloseout } from './contract'
import {
  daemonGet,
  daemonPost,
  daemonToday,
  formatHours,
  mondayOf,
  reviewRequest,
  workSeconds,
} from './lib'

type ReviewBlock = DaySummary['blocks'][number]

type ReviewState = {
  blocks: ReviewBlock[]
  selected: number | undefined
  editing: 'ticket' | 'description' | undefined
  error: string | undefined
}

const USAGE = 'usage: /worklog today|week|review'

const review = atom(
  { plugin: 'worklog', key: 'review' } as const,
  { blocks: [], selected: undefined, editing: undefined, error: undefined } as ReviewState,
)

function change($: EngineInterface, patch: Partial<ReviewState>): Promise<unknown> {
  return update($, review, state => ({ ...state, ...patch }))
}

async function makeIo($: EngineInterface): Promise<Io> {
  return {
    fetch: (url, init) => $.http.fetch(url, init),
    run: (argv, cwd) => $.process.run(argv, { cwd }),
    after: (milliseconds, callback) => $.clock.after(milliseconds, callback),
    now: () => $.clock.now(),
    home: await $.env.get('HOME'),
  }
}

async function loadReview($: EngineInterface, io: Io): Promise<void> {
  const today = await daemonToday(io)
  const summary = today.ok ? await daemonGet<DaySummary>(io, `/days/${today.value}`) : today
  if (!summary.ok) {
    await change($, { error: summary.error })
    return
  }
  const blocks = summary.value.blocks.filter(block => block.ignored_at === null)
  await update($, review, state => ({
    blocks,
    selected: blocks.some(block => block.id === state.selected) ? state.selected : blocks[0]?.id,
    editing: undefined,
    error: undefined,
  }))
}

async function sendReview(
  $: EngineInterface,
  io: Io,
  toAction: (block: ReviewBlock) => ReviewAction,
): Promise<void> {
  const { blocks, selected } = await read($, review)
  const target = blocks.find(block => block.id === selected)
  if (target === undefined) return
  const request = reviewRequest(target.id, toAction(target))
  const result = await daemonPost<unknown>(io, request.path, request.body)
  if (result.ok) await loadReview($, io)
  else await change($, { error: result.error })
}

async function summarise($: EngineInterface, io: Io, period: 'today' | 'week'): Promise<string> {
  const today = await daemonToday(io)
  if (!today.ok) return `worklog: ${today.error}`
  if (period === 'today') {
    const summary = await daemonGet<DaySummary>(io, `/days/${today.value}`)
    return summary.ok
      ? `worklog today: ${formatHours(workSeconds(summary.value.blocks))}`
      : `worklog: ${summary.error}`
  }
  const week = await daemonGet<WeekCloseout>(io, `/weeks/${mondayOf(today.value)}/closeout`)
  if (!week.ok) return `worklog: ${week.error}`
  const seconds = week.value.days.reduce((total, day) => total + day.logged_seconds, 0)
  return `worklog week: ${formatHours(seconds)}`
}

function typedAction(editing: 'ticket' | 'description', value: string): ReviewAction {
  return editing === 'ticket'
    ? { kind: 'ticket', jira_issue: value.trim() === '' ? null : value.trim() }
    : { kind: 'description', description: value }
}

function blockLabel(block: ReviewBlock): string {
  const personal = block.is_personal ? ' [personal]' : ''
  return `${block.started_at.slice(11, 16)} ${formatHours(block.duration_seconds)} ${block.jira_issue ?? '-'}${personal} ${block.description ?? ''}`
}

async function reviewPane($: EngineInterface, event: RenderInput<'Pane'>) {
  const { Box, Text, Button, Input, Select } = $.ui.resolve(event)
  const io = await makeIo($)
  const { blocks, editing, error, selected } = await read($, review)
  const current = blocks.find(block => block.id === selected)

  return (
    <Box flexDirection="column">
      {error !== undefined && <Text color="red">{error}</Text>}
      {current === undefined ? (
        <Text dimColor>No blocks today.</Text>
      ) : (
        <Box flexDirection="column">
          <Select
            key="block"
            value={String(current.id)}
            options={blocks.map(block => ({ value: String(block.id), label: blockLabel(block) }))}
            onSelect={value => change($, { selected: Number(value) })}
          />
          <Box>
            <Button key="personal" hotkey="p" onPress={() => sendReview($, io, block => ({ kind: 'personal', is_personal: !block.is_personal }))}>
              personal
            </Button>
            <Button key="ignore" hotkey="i" onPress={() => sendReview($, io, () => ({ kind: 'ignore', ignored: true }))}>
              ignore
            </Button>
            <Button key="ticket" hotkey="t" onPress={() => change($, { editing: 'ticket' })}>
              ticket
            </Button>
            <Button key="description" hotkey="d" onPress={() => change($, { editing: 'description' })}>
              description
            </Button>
          </Box>
          {editing !== undefined && (
            <Box>
              <Input
                key="edit"
                label={editing}
                value={(editing === 'ticket' ? current.jira_issue : current.description) ?? ''}
                onSubmit={value => sendReview($, io, () => typedAction(editing, value))}
                onCancel={() => change($, { editing: undefined })}
              />
              <Button key="cancel" onPress={() => change($, { editing: undefined })}>
                cancel
              </Button>
            </Box>
          )}
        </Box>
      )}
    </Box>
  )
}

export function registerCommand(on: On): void {
  on('session.start', { isInteractive: true }, async ($, event, next) => {
    await $.command.register({
      name: 'worklog',
      description: 'Show logged hours, or review today in a pane',
      argumentHint: 'today|week|review',
    })
    return next(event)
  })

  on('command.run', { command: 'worklog' }, async ($, event) => {
    const argument = event.args.trim()
    if (argument !== 'today' && argument !== 'week' && argument !== 'review') {
      return { text: USAGE }
    }
    const io = await makeIo($)
    if (argument !== 'review') return { text: await summarise($, io, argument) }
    await change($, { selected: undefined })
    await loadReview($, io)
    await $.ui.open({ id: REVIEW_PANE_ID, title: 'worklog review', focus: true, closeOnEscape: true })
    return { text: 'worklog review opened' }
  })

  on('ui.render', { component: 'Pane', requestId: REVIEW_PANE_ID }, ($, event) =>
    reviewPane($, event),
  )
}
