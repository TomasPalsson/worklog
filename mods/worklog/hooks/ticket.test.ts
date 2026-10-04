import { test, expect, mock } from 'claude-code/testing'
import type { On } from 'claude-code'

const WORK_CWD = '/home/owner/Desktop/Work/api'

const seat = (on: On, branch: { current: string | undefined }) => {
  const toasts: string[] = []
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
    const isBranch = e.argv.join(' ') === 'git branch --show-current'
    return isBranch && branch.current !== undefined
      ? { value: { exitCode: 0, stdout: `${branch.current}\n`, stderr: '' } }
      : { value: { exitCode: 1, stdout: '', stderr: '' } }
  })
  return { toasts, clock }
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
