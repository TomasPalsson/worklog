# Design — Claude Code terminal mod for worklog

Only what two tasks must agree on. Spec §-refs are to spec.md.

## 1. Contract file + language

Contract: `mods/worklog/hooks/contract.ts`, orchestrator-owned. Every mod task imports from it; none edits it.
Mod language: TypeScript ES module, run by the Claude Code engine (no Node, no DOM). Types come from `'claude-code'`: `Register`, `On`, `EngineInterface` (the `$`). Tests import `test, expect, mock` from `'claude-code/testing'`.
Async: every `$` call is async. Daemon helpers never throw; they resolve `DaemonResult<T>`.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| work context | `WorkContext` / `workContext()` | contract.ts / lib.ts | env, sessionInfo |
| local day | `LocalDay` | contract.ts | date, dayStr |
| ticket | `ticket` (mod), `jira_issue` (daemon field) | contract.ts | issue, key (as a field) |
| last workday | `lastWorkday()` | lib.ts | yesterday, prevDay |
| work seconds | `workSeconds()` | lib.ts | billable, total |
| review action | `ReviewAction` | contract.ts | op, mutation |

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| daemon HTTP | JSON body, status | `daemonGet` / `daemonPost` in lib.ts | `{ok:false, error}`: body's `error` field if present, else `HTTP <status>`, else the fetch error message; a call not settled within 2000 ms (raced against `io.after`) resolves `{ok:false, error:'timeout'}` |
| git subprocess | stdout | `workContext` | non-zero exit or throw → that field `undefined` / `isWork` from the path rule alone |
| hook payload `cwd` (Rust) | string or absent | `hook_run::handle` | absent → no branch lookup |

## 3. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| daemon base URL | `DAEMON_URL` in contract.ts | import | lib.ts only; T004–T006 call lib.ts, never `$.http.fetch` directly |
| once-a-day marker | `$.store` key `NUDGE_STORE_PREFIX + <LocalDay>` | — | T004 only |
| embedded mod files | `include_str!` list in `claude_mod.rs` | — | T002; the list below is closed |

Embedded file list (T002 embeds exactly these; mod tasks create no other runtime file): `.claude-plugin/plugin.json`, `hooks/hooks.json`, `hooks/register.tsx`, `hooks/contract.ts`, `hooks/lib.ts`, `hooks/status.ts`, `hooks/ticket.ts`, `hooks/command.tsx`, `types/index.d.ts` (added by T006: plugin.json names it as the types contract). Test files (`*.test.ts(x)`) are not embedded.

## 4. Module boundaries

- `hooks/contract.ts` · may import: nothing · exports: §1 types and constants.
- `hooks/lib.ts` · may import: `contract.ts`, types from `claude-code` · exports: the CALLS in T003's block, nothing else.
- `hooks/status.ts`, `hooks/ticket.ts`, `hooks/command.tsx` · may import: `contract.ts`, `lib.ts`, `claude-code` · each exports exactly one `(on: On) => void`. Siblings never import each other.
- `hooks/register.tsx` · imports the three registrars and calls each once. Nothing else.
- Rust `worklog-core/src/claude_mod.rs` · used only by `worklog-cli` (`cli.rs` hook handlers, `wizard.rs::configure_hook`). `hook.rs` does not call it (its tests sandbox only `$CLAUDE_HOME`).

## 5. Deliberately duplicated

- Jira key regex: Rust keeps `hook_run::jira_re`; the mod keeps `JIRA_KEY_RE`. Do not share across languages.
- `makeIo($): Io`: each of status.ts, ticket.ts and command.tsx declares its own local `function makeIo($: EngineInterface): Io` (closures: `fetch: (u, i) => $.http.fetch(u, i)`, `run: (argv, cwd) => $.process.run(argv, { cwd })`, `after: (ms, fn) => $.clock.after(ms, fn)`, `now: () => $.clock.now()`, `home: $.env.get('HOME')`). Never move it into lib.ts: the validator refuses `$` crossing an import (`$ is followed only into a function declared in this same file`).
- Work-folder rule: the mod re-implements "under ~/Desktop/Work/" in lib.ts instead of calling the Rust `billing::work_prefix`.

## 6. Decisions

- In the context of the local day, facing that `WORKLOG_TZ` may live only in worklog's `.env` file, we chose `daemonToday()` = `GET /logged?from=<utc day>&to=<utc day>` → `.today`, and rejected parsing `WORKLOG_TZ` in the mod, to keep one source of truth, accepting one extra request per poll. Makes hard: lib.ts.
- In the context of enabling the mod, facing that releases ship one binary, we chose embedding the files (`include_str!`, like `skill.rs`) and writing them to `<Paths data dir>/claude-mod`, registered via `env.CLAUDE_CODE_PLUGIN_DIRS` in `~/.claude/settings.json` (merged into any existing value, `:`-separated, no duplicates), and rejected `claude plugin marketplace add` because it needs the `claude` binary at install time. Makes hard: claude_mod.rs, cli.rs.
- In the context of hook-run, facing D-04, we chose `first_jira_key(prompt, cwd).or_else(branch key)`, calling `git::current_branch` only when that is `None`. Makes hard: hook_run.rs.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|

## Contract for T001 — hook-run branch fallback
CONTRACT   none (Rust only)
MODULE     rust/crates/worklog-core/src/hook_run.rs
CALLS      `crate::git::current_branch(&Path) -> Option<String>`; `first_jira_key(&[Option<&str>]) -> Option<String>` (existing)
DUPLICATE  Jira key regex stays in Rust.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — embed and enable the mod
CONTRACT   none (Rust only); embed exactly these 9 files from mods/worklog (closed list): .claude-plugin/plugin.json, hooks/hooks.json, hooks/register.tsx, hooks/contract.ts, hooks/lib.ts, hooks/status.ts, hooks/ticket.ts, hooks/command.tsx, types/index.d.ts — no *.test.ts(x)
MODULE     rust/crates/worklog-core/src/claude_mod.rs · exports: `pub fn install(data_dir: &Path) -> Result<PathBuf>`, `pub fn uninstall(data_dir: &Path) -> Result<()>`, `pub fn is_installed(data_dir: &Path) -> bool`, `pub const PLUGIN_DIRS_KEY: &str = "CLAUDE_CODE_PLUGIN_DIRS"`
CALLS      settings path + atomic write: reuse `crate::hook::settings_path()` and the same write pattern as `hook::write_settings` (make them `pub(crate)` if private); take `hook::CLAUDE_HOME_TEST_LOCK` in tests
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T003 — mod skeleton and lib
CONTRACT   mods/worklog/hooks/contract.ts — import from it.
NAMES      §1 table, verbatim.
MODULE     mods/worklog/hooks/lib.ts · exports exactly:
CALLS      `daemonGet<T>(io: Io, path: string): Promise<DaemonResult<T>>`
           `daemonPost<T>(io: Io, path: string, body: unknown): Promise<DaemonResult<T>>`
           `daemonToday(io: Io): Promise<DaemonResult<LocalDay>>`
           `workContext(io: Io, cwd: string): Promise<WorkContext>`
           `ticketFromBranch(branch: string | undefined): string | undefined`
           `lastWorkday(today: LocalDay): LocalDay` · `mondayOf(day: LocalDay): LocalDay`
           `workSeconds(blocks: readonly Block[]): number` · `formatHours(seconds: number): string` (`2h30`, `0h05`)
           `reviewRequest(blockId: number, action: ReviewAction): { path: string; body: unknown }`
           register.tsx: `export const register: Register = on => { registerStatus(on); registerTicket(on); registerCommand(on) }`
           stubs: status.ts / ticket.ts / command.tsx each `export function registerX(on: On): void {}`
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — status line and daily reminder
CONTRACT   mods/worklog/hooks/contract.ts — import from it.
MODULE     mods/worklog/hooks/status.ts · exports: `registerStatus(on: On): void`
CALLS      lib.ts: `daemonToday`, `daemonGet<DaySummary>('/days/<day>')`, `daemonGet<WeekCloseout>('/weeks/<monday>/closeout')`, `workContext`, `lastWorkday`, `mondayOf`, `workSeconds`, `formatHours`; `$.clock.every(POLL_MS, …)` started in `session.start`, first poll not awaited; `$.ui.status`, `$.ui.toast`, `$.store.get/set`
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T005 — branch ticket toast, context and trailer
CONTRACT   mods/worklog/hooks/contract.ts — import from it.
MODULE     mods/worklog/hooks/ticket.ts · exports: `registerTicket(on: On): void`
CALLS      lib.ts: `workContext`; hooks `session.start` (toast), `prompt.context` (append `{name:'worklog', text}` to `(await next(e)).blocks`, text byte-stable for a given ticket), `attribution.text` matcher `{kind:'commit'}` and `{kind:'pr'}` (append `\n\nTicket: <KEY>` to `(await next(e)).text`); re-read `workContext` on `turn.complete` without awaiting it in the turn path
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T006 — /worklog command and review pane
CONTRACT   mods/worklog/hooks/contract.ts — import from it.
MODULE     mods/worklog/hooks/command.tsx · exports: `registerCommand(on: On): void`
CALLS      `$.command.register({name:'worklog', description, argumentHint:'today|week|review'})` in `session.start`; `on('command.run', {command:'worklog'}, …)`; `$.ui.open({id: REVIEW_PANE_ID, title:'worklog review', focus:true, closeOnEscape:true})`; `on('ui.render', {component:'Pane', requestId: REVIEW_PANE_ID}, …)` with `Select` (block picker) + `Button` hotkeys `p` `i` `t` `d` + `Input` for ticket/description; lib.ts: `daemonToday`, `daemonGet`, `daemonPost`, `reviewRequest`, `mondayOf`, `workSeconds`, `formatHours`; pane state via `atom/read/update` from `claude-code`
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T007 — hours as a footer mode label
CONTRACT   mods/worklog/hooks/contract.ts — import from it. Engine API: ~/.local/share/worklog/claude-mod/.claude-plugin/types/claude-code/index.d.ts (grep `SessionMode: {`, `RenderInputOf`, `atom`).
NAMES      state value `worklog.hours: string | undefined` (the label text, e.g. `worklog 2h30`; undefined = no label). Declare it in mods/worklog/types/index.d.ts `PluginState.worklog` beside `review` (the contract must stay self-contained: no imports).
MODULE     mods/worklog/hooks/status.ts · still exports only `registerStatus(on: On): void`
CALLS      `const hours = atom({ plugin: 'worklog', key: 'hours' } as const, undefined as string | undefined)` (same pattern as command.tsx's `review` atom). `showHours` does `update($, hours, () => text)` instead of `$.ui.status(text)`; failure → `undefined`. New hook: `on('ui.render', { component: 'SessionMode' }, async ($, e, next) => { const text = await read($, hours); return next(text ? { ...e, props: { ...e.props, modes: [...e.props.modes, text] } } : e) })`. Remove every `$.ui.status` call. Reminder toast, poll interval and the work-hours count are unchanged.
TESTS      Replace status-line assertions with: render `SessionMode` (props `{ modes: ['focus'] }`) and assert modes end with `worklog 1h30` after the poll; personal blocks excluded; daemon failure → modes unchanged (no label). Find how command.test.tsx renders a component / reads state and mirror it.
THE FIVE   as above.

## Contract for T008 — rename the command to /wl
MODULE     mods/worklog/hooks/command.tsx · `$.command.register({ name: 'wl', … })`, `on('command.run', { command: 'wl' }, …)`, USAGE `usage: /wl today|week|review`. Output texts (`worklog today: …`, `worklog week: …`, pane title `worklog review`) unchanged.
TESTS      command.test.tsx: every `command: 'worklog'` / registered-name assertion / test title naming `/worklog` → `wl` / `/wl`. Add nothing else.
THE FIVE   as above.

## Contract for T009 — review pane layout
MODULE     mods/worklog/hooks/command.tsx · only `reviewPane` and `blockLabel` change. Keep every element `key`, `hotkey`, `onPress`/`onSelect`/`onSubmit`/`onCancel` behaviour, the Select (arrow-key navigation), and the request sent per action exactly as now.
LAYOUT     Engine props: ~/.local/share/worklog/claude-mod/.claude-plugin/types/claude-code/index.d.ts (`BoxProps` ~l.841, `ButtonProps` ~l.1000 — `plain` draws `p: label`, `TextProps` ~l.12011).
           Outer `<Box flexDirection="column" borderStyle="round" borderDimColor paddingX={1}>`.
           Header `<Box justifyContent="space-between">`: left `<Text bold>{day}</Text>` (the blocks' `day`, else omit), right `<Text dimColor>work {formatHours(workSeconds(blocks))} · personal {formatHours(sum of duration_seconds of is_personal blocks)}</Text>`. Reuse lib `formatHours`/`workSeconds`.
           Rows: the Select, `marginY={1}` around it. `blockLabel(block)` = `${HH:MM start}–${HH:MM end}  ${formatHours(duration).padStart(5)}  ${(jira_issue ?? '—').padEnd(12)}  ${is_personal ? 'personal  ' : ''}${description truncated to 40 chars with '…'}` trimmed at the end. start/end from `started_at`/`ended_at` `.slice(11, 16)` as today.
           Actions `<Box gap={3}>`: the four Buttons get `plain`; the personal Button's label reads `work` when the block is personal, else `personal` (key stays `personal`).
           Editing row `marginTop={1}`, `gap={2}`; cancel Button `plain dimColor`.
           Error: `<Text color="red">` directly under the header. Empty day: header-less bordered box with `<Text dimColor>Nothing to review today.</Text>`.
TESTS      Update only assertions the layout breaks; add one test that the Select labels are aligned (e.g. label of a ticketless personal 35-min block equals `11:44–12:19   0h35  —             personal` per the formula) and one that the personal Button label reads `work` for a personal block. Existing behaviour tests (hotkeys, POST bodies) must stay and pass unchanged.
THE FIVE   as above.
