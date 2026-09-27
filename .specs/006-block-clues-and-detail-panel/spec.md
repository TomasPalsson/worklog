# Spec: Block clues, attribution and detail view

**Created**: 2026-09-27 · **Route**: dispatch · **Prep**: PREP.md (16 questions, D-01..D-14)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Blocks are assembled by the clock, not by content: every commit and PR has no folder, so it is glued into whatever project is active that minute — worklog and flow commits land in billable vitinn-infra and LibreChat blocks. The owner cannot see what really happened inside a block, and the invoice line their bosses and customers read ("Explore Claude agent tools and configuration") says nothing.

**Solution**: Four phases in one spec. **A** puts events in the right project (repo → folder, no personal GitHub, no foreign-machine commits, no clock-glue). **B** stores everything the existing sources see, locally, secret-scrubbed. **C** adds a Details view that shows all of it on one filterable timeline. **D** writes one Icelandic, plain-language text per billing line from a minimal, scrubbed set of clues.

**Who it's for**: the owner (one person, one Mac), and downstream their bosses and customer businesses who read the invoice text.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: an event decides a block's project only through its own folder (or its repo's mapped folder); time proximity may place an event inside a block but never decides a project, extends a block, or bills foreign work.

## Decisions (verbatim from PREP.md — never re-asked)

- D-01 One spec, four phases in order: A attribution fix (repo→folder, drop commits not on this machine, no clock-glue for folderless events) → B collect everything locally → C side-panel detail view → D better descriptions. Phase A ships and is checked on a real day in its own PR before B starts. Spec must be extensive so nothing from this prep is lost. — user, Q1
- D-02 What leaves the machine (description writer, `claude -p`) follows three tests the user set — is it needed, could it harm the company, can a less risky field say the same. Result (user delegated the per-field call to Mr Claude, Q2): SEND time/duration, project folder, branch, commit + PR first line, Jira key + candidate ticket titles, file basenames, shell program names, browser domain only, Slack channel name. NEVER SEND repo name, commit/PR bodies, full shell commands, Claude prompt text, tool inputs/outputs, raw_json, full URLs, page titles, Slack message text, DM counterpart names, inter-session messages, personal blocks. Everything sent passes a secret scrub (tokens, keys, emails, IPs, account ids). Accepted cost: exploration-only blocks get weaker descriptions. — user, Q2
- D-03 Phase B scrubs secrets (tokens, keys, passwords) BEFORE writing to worklog.db; everything else is stored raw locally. — user, Q3
- D-04 Phase B copies Claude activity into worklog.db: full prompt text and tool inputs, each tool output truncated to its first 2 KB (~1 MB/day); kept until the normal purge — not pointer-only (transcripts expire ~30 days). — user, Q4
- D-05 Subagent / sidechain / background-job / `agents` teammate activity is collected and shown in the side panel, but adds NO time to any block — block time stays the owner's own time. — user, Q5
- D-06 No github_commit / github_pr from the owner's personal GitHub account (repos owned by `TomasPalsson`, i.e. the configured github_user) is tracked at all. Only org repos (e.g. aproorg/*) are collected. — user, Q6
- D-07 An org commit/PR not made on this machine (no local clone under ~/Desktop/Work, or the sha is absent from that clone) is never put in a block and never billed; it is listed in a per-day "done elsewhere" list, from which the owner can move it into a block by hand. — user, Q7
- D-08 A folderless event (e.g. shell `cd`/`agents` run from ~) may join a block only inside that block's existing time span; it never votes on the block's project and never extends the block's start/end. — user, Q8
- D-09 When phase A ships, every day still in worklog.db is re-inferred under the new rules (at prep time: 117 blocks, 2026-09-21..27, 0 Tempo-synced, 0 exported, 4 manual — manual descriptions survive via the existing carry). — user, Q9
- D-10 The inline "What happened" list stays as the quick look; a new "Details" control opens a full-detail view of the block. Its form (side panel, full page, modal) is Mr Claude's/the spec's call — it holds a lot of information. — user, Q10
- D-11 The Details view is ONE time-ordered timeline with per-source filters (Claude, shell, git, browser, Slack, …); Claude sessions fold into prompt → tool calls → files touched, with subagent/teammate work nested under its parent session; clicking any row shows its full stored data. — user, Q11
- D-12 Phase D's target is the SUPER BLOCK description — the billing line (folder × customer per day) whose text goes to customers and bosses as `Texti á reikning` — not the small per-block lines. It must say concretely what was done ("Explore Claude agent tools and configuration" is the failure example). Small-block descriptions become inputs to it. — user, Q12
- D-13 Super-block text = a couple of plain sentences explaining what was done. No PR numbers, no ticket numbers (and, per D-02, no tool names or file paths). — user, Q13
- D-14 Super-block text is written in Icelandic. — user, Q14

## 1. Context

### 1.1 Problem statement

On 2026-09-24/25 the owner found worklog, flow and ads-seo commits inside billable work blocks, a `test(daemon)` commit inside a LibreChat block, and a `fix(infer)` commit inside a vitinn-infra block. The events that would explain a block — full shell commands, Claude prompts and tool calls, subagent work — are thrown away at collection time, so the owner cannot reconstruct what happened. The invoice line, which bosses and customers read, is a `;`-joined list of vague per-block lines.

**Current workaround**: fix each block's customer by hand ("set by you"), and rewrite invoice text by hand or send it vague.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | runs worklog, reviews days, bills | one person, one Mac, many repos and worktrees, runs `agents` teams |
| Invoice reader | boss / customer business reading `Texti á reikning` | Icelandic, non-technical, sees only the billing line |

**Primary actor**: Owner.
**Hidden stakeholders**: the invoice reader; the company, whose information must not leave the machine beyond D-02.

## 2. Scope

### 2.1 In scope

- **Phase A — attribution**: repo → local folder for commits/PRs; personal-account repos never collected; org commits absent from this machine go to a "done elsewhere" list; folderless events can sit inside a block but never decide or stretch it; one-off re-inference of every stored day.
- **Phase B — capture**: a secret scrubber applied before storage; every existing collector stores what it already reads (full shell command + cwd, full reflog message, hook payload, prompt text, tool inputs, tool outputs ≤ 2 KB, sidechain/subagent/teammate/background-job activity, inter-session messages) in the event's raw record.
- **Phase C — Details view**: a "Details" control per block opening a full-detail view: one timeline, per-source filters, Claude sessions folded prompt → tools → files, helpers nested under their parent, any row expandable to its full stored record.
- **Phase D — super-block text**: one generated Icelandic text per billing line, from a minimal scrubbed input (D-02); owner-editable; hand edits never overwritten; the per-block writer also switches to the D-02 input.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No fine-tuning of the Verdict model and no training-data export — a later spec, once the data is clean.
- No new data sources (gcal, knowledgeC/Screen Time, Firefox places.sqlite backfill, …); phase B only stores more from existing sources.
- Nothing new leaves the machine beyond the D-02 send-list.
- No change to billing maths (interval union, round_to_half_hour) or to Tempo sync.
- No restyle of the rest of the day page; only the new Details view is new UI.

## 3. Journeys

### Journey 1 — Commit lands in the right place (Owner) — phase A

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | an `aproorg/code-interpreter` commit whose sha exists in `~/Desktop/Work/code-interpreter` at 10:10 while a vitinn-infra block is active | the day is re-inferred | the commit sits in a code-interpreter (or its submodule-owner's) block, not vitinn-infra |
| Error | a `TomasPalsson/worklog` commit on GitHub | collection runs | nothing is stored; no block, no "done elsewhere" row |
| Edge | an `aproorg/*` commit whose sha is not in any local clone | the day is re-inferred | it appears only in that day's "done elsewhere" list; no block time, no billing line |
| Edge | shell `agents` run from `~` at 10:09 inside a vitinn-infra block 10:08–10:36 | the day is re-inferred | it shows in that block; the block still starts 10:08 and is still vitinn-infra |
| Edge | shell `cd` from `~` at 12:00 with no block spanning 12:00 | the day is re-inferred | it joins no block and creates none |

### Journey 2 — See exactly what happened (Owner) — phases B + C

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a block with a Claude session, 3 subagent runs, shell commands and a commit | the owner opens Details | one timeline; the session folds into prompt → tool calls → files; subagent runs nest under it; each row expands to its full stored record |
| Error | a shell command containing `ghp_…` token | it is collected | the stored command reads `[secret]` where the token was; the token is nowhere in worklog.db |
| Edge | a tool output of 40 KB | it is collected | the first 2 KB are stored, with a marker saying how much was cut |
| Edge | the owner filters to "shell" only | clicks the filter | only shell rows remain; turning it off restores the full timeline |

### Journey 3 — An invoice line a boss understands (Owner → Invoice reader) — phase D

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | the vitinn-infra billing line for 2026-09-25 | the super-block text is generated | 2–3 Icelandic sentences saying what was done, no PR/ticket numbers, no file paths |
| Error | the description writer is unreachable or fails | generation runs | the line keeps its previous text (or today's fallback), and the UI says the text is not generated |
| Edge | the owner edited the line's text by hand | generation runs again | the hand-edited text is unchanged |
| Edge | a block's only clue is a Claude prompt (exploration, no commits) | generation runs | the prompt is not sent; the text is built from branch, folder and file names only |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The system MUST NOT store any commit or PR from a repo owned by the configured personal GitHub account | collector test with a personal-owner fixture stores 0 rows |
| FR-02 | MUST | On upgrade, the system MUST delete stored commit/PR events from personal-owner repos | migration test: rows gone, org rows intact |
| FR-03 | MUST | The system MUST give an org commit/PR the local folder of the clone that contains its sha (direct clone, or submodule owner — see Glossary) | test: sha present in fixture clone → folder set; submodule fixture test |
| FR-04 | MUST | The system MUST mark an org commit/PR whose sha is in no local clone as "done elsewhere" and keep it out of every block | test: absent sha → no block_events row, flagged elsewhere |
| FR-05 | MUST | The Owner MUST see a per-day "done elsewhere" list | UI test renders the list for a fixture day |
| FR-06 | MUST | The Owner MUST be able to move a "done elsewhere" item into a chosen block | action test: item linked to block, leaves the list |
| FR-07 | MUST | A folderless event MUST NOT decide any block's project | infer test: folderless event never changes lane owner |
| FR-08 | MUST | A folderless event MUST NOT extend a block's start or end | infer test: block bounds unchanged by it |
| FR-09 | MUST | A folderless event outside every block's span MUST join no block | infer test |
| FR-10 | MUST | On upgrade, the system MUST re-infer every stored day once, keeping Tempo ids, invoiced markers, manual descriptions and tickets | migration test on a fixture DB: carry fields preserved, `exported_at` included |
| FR-11 | MUST | The system MUST scrub secrets from every value before it is stored | scrubber unit tests over token/key/password/private-key fixtures |
| FR-12 | MUST | The system MUST store the full shell command line and its working directory per shell event | collector test |
| FR-13 | MUST | The system MUST store the full reflog message per git event | collector test |
| FR-14 | MUST | The system MUST store Claude prompt text and every tool call's input per Claude event | collector test on a transcript fixture |
| FR-15 | MUST | The system MUST store each tool output truncated to 2 KB with a cut marker | collector test with a 40 KB output |
| FR-16 | MUST | The system MUST store subagent, sidechain, background-job and teammate activity linked to its parent session via the session/turn id recorded in the Claude transcript (never by shared cwd) | collector test on a sidechain fixture and a two-teammates-one-cwd fixture |
| FR-17 | MUST | Helper activity (FR-16) MUST add no time to any block | infer test: block duration equal with and without helper rows |
| FR-18 | MUST | The system MUST store inter-session (teammate) messages as their own kind of event | collector test |
| FR-19 | MUST | The Owner MUST be able to open a Details view from each block | UI test: control present, opens view |
| FR-20 | MUST | The Details view MUST list every event of the block, including helper activity, in time order | UI test on fixture |
| FR-21 | MUST | The Details view MUST filter by source | UI test |
| FR-22 | MUST | The Details view MUST fold a Claude session into prompt → tool calls → files touched | UI test |
| FR-23 | MUST | The Details view MUST nest helper activity under its parent session | UI test |
| FR-24 | MUST | Any Details row MUST expand to its full stored record | UI test |
| FR-25 | MUST | The inline "What happened" list MUST remain as today | existing EventList tests stay green |
| FR-26 | MUST | The system MUST generate one text per billing line (day × folder × customer) | test: one stored text per line key |
| FR-27 | MUST | The billing-line text MUST be 2–3 Icelandic sentences with no PR numbers, ticket numbers, file paths or tool names | prompt/output-validator test; rejected outputs fall back |
| FR-28 | MUST | The description writer MUST receive only the D-02 send-list, scrubbed | test: serialized request contains no forbidden field (fixture with prompt text, URL, Slack text, commit body) |
| FR-29 | MUST | The per-block description writer MUST also use only the D-02 send-list | same test over the per-block request |
| FR-30 | MUST | The Owner MUST be able to edit a billing-line text by hand | UI + action test |
| FR-31 | MUST | A hand-edited billing-line text MUST never be overwritten by generation | test |
| FR-32 | MUST | The invoice export MUST use the billing-line text when one exists, else today's joined/fallback text | export test |
| FR-35 | MUST | When the description writer fails, is unreachable or its output fails validation, the billing-line text MUST keep its previous value and the Owner MUST see that it was not (re)generated | test: simulated writer failure → text unchanged, UI shows the not-generated flag |
| FR-33 | SHOULD | The Owner SHOULD be able to regenerate a billing-line text on demand | UI test |
| FR-34 | MAY | The Details view MAY link a commit row to its GitHub page | UI test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Stored growth from phase B | ≤ 5 MB per active day (target ~1 MB) | `worklog.db` size before/after collecting 2026-09-25 |
| Tool-output cap | 2048 bytes per output | collector test |
| Details view load | < 1 s for a block with 1 000 events, local | timed fetch in a test fixture |
| Secrets in worklog.db after phase B | 0 matches for the scrubber's own patterns | `sqlite3` grep over events after collecting a seeded fixture |
| Data leaving the machine | 0 fields outside the D-02 list | request-capture test (FR-28) |
| Billing-line text length | 2–3 sentences, ≤ 400 characters | validator in FR-27 |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test.
- [ ] Phase A merged in its own PR and checked by the owner on a real re-inferred day before phase B starts (D-01).
- [ ] The error path of each journey is exercised.
- [ ] §5 numbers measured on real data, not assumed.
- [ ] **Owner's check (PREP Verify)**: open 2026-09-25 after all four phases ship: no worklog/flow/ads-seo (personal-account) commit or PR sits in any work block; the vitinn-infra block's Details view shows every Claude prompt, tool call and shell command in its span; its billing line reads as 2–3 Icelandic sentences the owner would send to their boss unedited.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Blocks are built by time lanes; a folderless event matches ANY lane (`is_none_or`) — infer_lanes.rs:105-107, infer.rs:300 | High | Phase A fix lands in the wrong place |
| A2 | github_commit/pr store `repo`, never `project_path`; repo→folder exists only in billing.rs:232-282 | High | duplicate mapping logic |
| A3 | 66 worklog + 5 flow events were glued into blocks on 09-24/25 | High | none — motivating evidence |
| A4 | ads-seo, aws-exam, homelab have no local clone | Med | moot after D-06 (personal) |
| A5 | claude_work already carries the right folder + branch | High | Details view nesting needs more work |
| A6 | raw_json is NULL everywhere; hook payload, full shell command, reflog tail, prompt text, sidechain turns are dropped on purpose — hook_run.rs:116-120, fish.rs:1-8, reflog.rs:9-12, claude_transcripts.rs:12-17,333 | High | phase B scope grows |
| A7 | Descriptions come from `claude -p` in estimate.rs with 200/800-char caps, 400 events, 50 commits | High | phase D touches different code |
| A8 | "What happened" is an inline lazy list; no drawer primitive exists; raw_json is typed but unrendered in web | High | phase C needs a primitive first |
| A9 | Web reads go through the daemon HTTP API; CLAUDE.md's "bun:sqlite" line is stale | High | Details data path |
| A10 | `agents` runs `claude agents` (Agent Teams); teammates share one cwd; 7 launches on 09-25 | High | teammate nesting keyed wrongly |
| A11 | The `claude` hook stores raw cwd (9 paths) while transcripts collapse worktrees (4 paths) | High | inconsistent folders → splits |
| A12 | Worktree path → repo: strip `/.claude/worktrees/<name>`; basename is the branch | High | wrong folder for worktree events |
| A13 | Today the estimator already sends prompt text (≤800), Slack text, PR bodies, URLs (≤200) off-machine; only `redact_code` runs | Med | phase D is also a leak fix — must ship |
| A14 | D-06 is enforced in the collector and existing personal rows are deleted once (FR-01/02) | Med | personal commits reappear |
| A15 | Re-inference carry omits `exported_at` (infer.rs:470-545) — FR-10 must add it | Med | invoiced marker lost on rebuild → purge deletes billed work |
| A16 | Billing-line text today = distinct block descriptions joined "; ", ≤400 chars + "(+N more)" — billing.rs:508-555; billing lines are computed, not stored | High | phase D needs a new stored text keyed by line |
| A17 | A hand-edited billing-line text is never overwritten (mirrors `estimated_by='manual'`) | Med | owner's edits lost |
| A18 | The Details view is a dedicated per-block page (D-10 discretion) | Med | layout rework only |
| A20 | Re-inference and a billing-line text edit do not race: line texts live in their own table keyed by line, which re-inference never writes (judge note, not a requirement) | Med | an edit lost during a rebuild |
| A19 | Local sha presence is checked with the git binary against clones under ~/Desktop/Work only (org work lives there) | Med | org clones under ~/Desktop/Projects missed |

## 8. Open questions

1. [NEEDS CLARIFICATION: billing-line key — lines are keyed day × folder × customer today; when a customer split (multi-tenant) or ticket-mixed group changes after a text was generated, does the text follow the new key or reset? Default: reset to generated, never touch a hand edit.]

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Block | a stretch of the owner's time attributed to one project folder |
| Super block / billing line | all blocks of one day × folder × customer; one invoice-form submission |
| Folderless event | an event with no project folder of its own (shell from ~, unmapped) |
| Done elsewhere | an org commit/PR whose sha is in no local clone |
| Submodule owner | the top-level work folder whose `.gitmodules` declares the commit's repo as a submodule (the existing billing submodule map, e.g. `aproorg/code-interpreter` → `vitinn-infra`); the commit's folder is that top-level folder, and sha presence is checked in the submodule's checkout inside it |
| Helper activity | subagent, sidechain, background job, `agents` teammate work |
| Scrub | replace a detected secret with `[secret]` before storing or sending |
| D-02 send-list | the only fields allowed to leave the machine |
