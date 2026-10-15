# Spec: Topic atlas

**Created**: 2026-10-11 · **Route**: dispatch · **Detail**: plan.md (computations, chart forms), prototype.md (measured numbers), design-system.md (page tokens)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Statistics page's Topics section (#160) lumps 84% of blocks into one "topic", names it after one ticket ("LibreChat Fork … 82h 51m"), repeats ticket hours the Owner already has in Tempo, and shows four unexplained text lists. The Owner can't tell where the numbers come from.

**Solution**: A "Topic atlas" section: honest themes (centered-vector clustering, largest ≤ 25% on real data), named by AI from the work itself, shown as six graphics — a semantic galaxy map, theme heartbeats, a theme river, a day-similarity mirror, a focus × novelty compass, and first-times / oddballs — each with a visible "How" line.

**Who it's for**: the Owner, reviewing what their weeks were actually about.

**MVP cut line**: every `MUST` in §4.1 ships.

**Key decision**: subtract the window's mean vector before clustering (embeddings are anisotropic: mean pairwise cosine 0.786); AI names come from the existing model invoker, cached by member set so a page load almost never waits on a model.

## 1. Context

### 1.1 Problem statement

On the live DB the current clustering puts 167 of 198 blocks in one cluster (prototype.md §1: largest 84.3%, ticket AMI 0.088). Names are copied from the top ticket's summary, so the label says nothing about the other 160 blocks. Ticket hours duplicate Tempo.

**Current workaround**: none; the Owner ignores the section.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Opens /stats to see what the period was about | Single user; reads it on a laptop, sometimes a phone |

**Primary actor**: Owner. **Hidden stakeholders**: the classifier helper (embed) and the model invoker (naming), both shared with other features.

## 2. Scope

### 2.1 In scope

- Clustering on centered vectors, up to 10 topics.
- c-TF-IDF keywords and representative examples per topic.
- AI topic names + one-sentence descriptions, cached in SQLite, keyword fallback.
- 2-D semantic map (exact t-SNE, in Rust).
- Per-block novelty and oddball scores; per-day focus, novelty, switches; day-by-day similarity.
- Six graphics replacing the current Topics section, each with a "How" line and hover cards.
- Custom graphics (icons, empty states) from the design:graphics skill.

### 2.2 Non-goals

> Binding.

- No hours-per-ticket chart; ticket keys appear only as context, never as the measure.
- No writes outside `block_embeddings` and the new `topic_names` cache.
- No change to Tempo, billing, blocks or tickets.
- deferred: weekly drift / attention comet trail (noisy on 3 weeks of data).
- deferred: ticket ↔ text alignment, chord transitions, Foote novelty curve.

## 3. Journeys

### Journey 1 — See what the period was about (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | helper up, ≥10 described blocks | Owner opens /stats | Topic atlas shows a galaxy of named islands, heartbeats per theme, a river, a day mirror, a compass and first-times / oddballs, each with a "How" line |
| Error | helper off or down | Owner opens /stats | the section says "Topic stats need the local helper (`worklog verdict`)." and the rest of the page renders |
| Edge | names not cached yet | Owner opens /stats | keyword names show at once with an "AI is naming topics…" chip; a refresh ~25 s later shows AI names |
| Edge | no model configured | Owner opens /stats | keyword names show, no chip |
| Edge | 6–9 described blocks | Owner opens /stats | everything except the map; the map says it needs 10 blocks |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Topics MUST be formed from vectors centered on the range's mean | anisotropic fixture: two groups sharing a large common component split ≥ 0.9 by group |
| FR-02 | MUST | On the live-data copy, no topic MUST hold more than 25% of blocks | replay on the DB copy prints largest share |
| FR-03 | MUST | Each topic MUST carry up to 10 keywords from its own blocks' text, with Jira keys removed | unit test |
| FR-04 | MUST | Each topic MUST show an AI-written name and description when a model is available, else its top keywords | route test with a test invoker; invoker build error → keywords |
| FR-05 | MUST | A topic name MUST be reused, without a model call, when the topic's blocks are unchanged or overlap ≥ 60% with a named set | counter on the test invoker = 0 on the second load |
| FR-06 | MUST | Naming MUST NOT delay the page: missing names show keywords and a "naming" state | route test: first GET pending, later GET ai |
| FR-07 | MUST | The map MUST place similar blocks near each other (t-SNE), deterministically | two runs bit-identical; separated groups stay separated |
| FR-08 | MUST | Each block MUST get a novelty score (1 − nearest earlier block) and an oddball flag (nearest block in range < 0.30) | unit tests |
| FR-09 | MUST | Each day MUST get focus (None with < 2 blocks), novelty, switches, longest run, top topic | unit tests |
| FR-10 | MUST | Day-by-day similarity MUST be symmetric with a diagonal of 1, ≤ 62 days | unit test |
| FR-11 | MUST | The river MUST give hours per topic per day (per week above 90 days), columns summing to topic hours | unit test |
| FR-12 | MUST | The section MUST render the six graphics described in plan.md §1 with a visible "How" line each | component tests + Chrome check |
| FR-13 | MUST | Every mark MUST have a hover card, keyboard focus and an accessible label | component tests |
| FR-14 | MUST | Nothing MUST be written outside `block_embeddings` and `topic_names` | table-snapshot test |
| FR-15 | MUST | The helper-down and too-few-blocks states MUST keep working as today | existing tests + Chrome |
| FR-16 | MUST | The drift, sprawl, similar-days and ticket-named topic output MUST be gone | grep finds none |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Warm /stats/topics (names cached) on the DB copy | < 3 s | curl timing, release build |
| t-SNE at n = 2000 | < 10 s | `#[ignore]` bench, release |
| Model calls on an unchanged second load | 0 | test-invoker counter |
| Phone width | no horizontal scroll at 375 px | Chrome check |
| Colours | no hard-coded hex in new CSS | grep |

## 6. Launch criteria

- [ ] Every MUST has a passing test
- [ ] Gates: cargo fmt / clippy / test, bun test / typecheck / build
- [ ] Owner-facing check in Claude in Chrome on the installed app: all six graphics, hover cards, light and dark, 375 px
- [ ] Design evaluator subagent composite ≥ 4.0
- [ ] Test seams: Rust unit + daemon route tests with a mock helper and test invoker — catch math, caching, degrade / miss model name quality. bun component tests — catch rendering, tips, a11y attributes / miss real visual quality (Chrome + evaluator cover that).

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | The Owner's /goal message approves this plan ("make this look as amazing as possible … make actually useful data") | High | whole plan |
| A2 | Average-linkage on centered vectors with k = clamp(round(√(n/2)), 2, 10) gives coherent themes (prototype: largest 21.7%, 7 of 8 coherent) | High | odd themes; display-only |
| A3 | Haiku via the existing invoker names topics well with the prototype's prompt (8/8 good names, "only facts" rule stops invented facts) | Med | weaker names; keyword fallback |
| A4 | Exact t-SNE in Rust is fast enough for ≤ 2000 blocks | Med | slow map on huge ranges; bench gates it |
| A5 | Topic atlas replaces the Topics section in place | High | one-line move |

## 8. Open questions

None.
