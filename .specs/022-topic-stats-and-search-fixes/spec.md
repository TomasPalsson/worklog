# Spec: Topic stats and search fixes

**Created**: 2026-10-11 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: `worklog ask` returns nothing for 94% of real questions, because every word must match. The Owner clicks through hundreds of unsorted browser groups that they have already dismissed many times before. And the Statistics page counts hours but never says what the work was about.

**Solution**: Ask finds the right block in its top 5 for at least 80% of real questions (85.7% measured). The day view flags browser hosts the Owner always dismisses and offers one click to dismiss them. The Statistics page gains a Topics section: hours per topic, context switches, drift since last period, the catch-all tickets with mixed work, and the past days most like today.

**Who it's for**: the Owner (single user of worklog).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: the embedding model is used only for display-only topic stats. Every test where it guessed a ticket, a folder or a customer came out weak or failing (evidence: `~/Desktop/Projects/worklog-embed-trial/RESULTS.md`), so it writes nothing that lands on a block, ticket, invoice or Tempo.

## 1. Context

### 1.1 Problem statement

Ask joins words with AND and sorts by recency only. On 35 real Tempo line texts, the right block was in the top 5 only 5.7% of the time, and 94% of queries returned nothing. In the day view, 534 of 885 labelled browser events were dismissed one group at a time, many from hosts with no other kind of label ever. The Statistics page (#154–#156) shows when and how much the Owner worked, but nothing about which themes the time went to.

**Current workaround**: for ask, scroll the day view or grep. For noise, click dismiss on every group daily. For topics, nothing.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews their day, asks "when did I do X", reads their stats | Single user; also the maintainer |

**Primary actor**: Owner.
**Hidden stakeholders**: the classifier helper's existing callers (folder routing, ticket pick), which share its single thread.

## 2. Scope

### 2.1 In scope

- Ask: any-word matching with English and Icelandic stopwords dropped, best match first.
- Day view: a "dismissed N× before" tag on browser groups from always-dismissed hosts, and one button to dismiss all of them.
- Classifier helper: a text-embedding endpoint.
- Stored block vectors, computed lazily and refreshed when a block's text changes.
- Statistics page Topics section: theme map, context switches per day, drift vs the previous period, catch-all tickets and days like today.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Does NOT use embeddings to pick or suggest tickets, folders, customers, deildir or line text. Each was tested and came out weak or failing.
- Does NOT write topic results to blocks, tickets, billing, Tempo or rules. Topics are display-only.
- Does NOT auto-dismiss anything, and does NOT create ignore rules from the noise tag.
- Does NOT add a "nothing found" floor to ask (tested and failing).
- deferred: meeting gravity (gcal coverage is 11 events total, too few).
- deferred: a semantic ask fallback (it gave +0–3 pp, below the bar).
- deferred: topics for Slack, gcal or shell-only blocks with no text.

## 3. Journeys

### Journey 1 — Ask finds past work (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a block described "Configure Geitin visibility for VÍS staff" | Owner runs `worklog ask geitin access for vís` | that block is among the 5 hits, best match first |
| Error | a query made only of stopwords ("the and of") | Owner asks | the words are used as typed; no crash, normal "no matches" if none |
| Edge | two blocks match equally well | Owner asks | the newer one comes first |

### Journey 2 — Clearing browser noise (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | 3 unsorted groups from hosts dismissed ≥2× before and never labelled otherwise | Owner opens the day | each shows "dismissed N× before"; a "Dismiss N noise groups" button appears; one confirm dismisses exactly those groups |
| Error | a dismiss call fails mid-way | Owner confirms | a toast names the failure; groups already dismissed stay dismissed; the rest stay unsorted |
| Edge | a host was dismissed 5× but filed once | Owner opens the day | no tag on that host |

### Journey 3 — Topics on the Statistics page (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | the helper is running and the range holds ≥6 work blocks with text | Owner opens /stats | a Topics section lists 2–8 named topics with hours, per-day switch counts, drift rows, up to 3 catch-all tickets, and up to 3 similar days |
| Error | the helper is off or down | Owner opens /stats | the Topics section shows "Topic stats need the local helper (`worklog verdict`)"; every other section renders as before |
| Edge | the range holds 5 work blocks with text | Owner opens /stats | the Topics section shows "Not enough described work in this range yet" |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Ask MUST return blocks that match any query word, not only all words | a 2-word query with one word absent still finds the block |
| FR-02 | MUST | Ask MUST ignore English and Icelandic stopwords unless the query has nothing else | "the invoice" ranks like "invoice"; "the and" still runs |
| FR-03 | MUST | Ask MUST order hits by full-text relevance (bm25 rank), then newest first | a 3-word match outranks a newer 1-word match |
| FR-04 | MUST | Ask MUST keep treating operator characters as plain text and MUST NOT change any block, event or ticket | the existing hostile-query and search-changes-nothing tests pass |
| FR-05 | MUST | The day view MUST tag an unsorted browser group whose host has ≥2 dismissed browser events on earlier days and no other label there | tag shows the dismissed count |
| FR-06 | MUST | The Owner MUST be able to dismiss every tagged group with one confirmed click, with no ignore rule created | the dismiss calls carry no rule kind |
| FR-07 | MUST | A tagged group MUST keep its normal per-group actions (file, dismiss, rule) | existing group tests pass |
| FR-08 | MUST | The helper MUST return one unit-length 768-number vector per text it is given | an /embed self-test checks the length and the norm |
| FR-09 | MUST | Block vectors MUST be recomputed when the block's text or the model changes, and never otherwise | a second stats load sends 0 texts |
| FR-10 | MUST | The Statistics page MUST show 2–8 topics with names and hours for the range; a topic is named by the summary of its ticket with the most hours, else by its longest block's description | fixture names match the rule |
| FR-11 | MUST | The Statistics page MUST show per-day topic switches and the longest single-topic run | counts match a hand-made fixture |
| FR-12 | MUST | The Statistics page MUST show each topic's hours now vs the previous equal-length range | a fixture's before/after seconds match |
| FR-13 | MUST | The Statistics page MUST show up to 3 tickets whose blocks are least alike | the widest-spread fixture ticket ranks first |
| FR-14 | MUST | The Statistics page MUST show up to 3 earlier days most like the last day of the range, by cosine similarity of hour-weighted mean block vectors | a fixture's twin day ranks first |
| FR-15 | MUST | When the helper is unavailable, the Topics section MUST say so and all other stats MUST render | daemon test with the helper down returns `helper_down`; the page test renders the message |
| FR-16 | MUST | Topic computation MUST NOT write to any table except the block vector store | a table snapshot before and after is equal |
| FR-18 | MUST | When the range has fewer than 6 work blocks with text, the Topics section MUST say "Not enough described work in this range yet" | status `too_few_blocks` renders the message |
| FR-19 | MUST | When the helper returns the wrong number of vectors or a vector of the wrong length, the Topics section MUST show the helper-down message and nothing is stored | mock helper returning a short vector → `helper_down`, vector store unchanged |
| FR-17 | SHOULD | A topic row SHOULD show its top tickets on hover | hover-card test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Ask quality | hit@5 ≥ 80% and empty < 10% on the Tempo line-text set | `uv run ask-bm25-or-check.py`-style replay against the built binary (CHK) |
| Noise tag precision | ≥ 97% of tagged events were later dismissed, past-only replay | measured 97.7% precision / 78.1% recall at N=2 on the live DB |
| /stats/topics, warm cache | p95 < 2 s for a 30-day range | daemon timing log |
| /stats/topics, cold (≤300 blocks to embed) | < 60 s; web timeout 90 s | first load after install |
| /embed batch | ≤ 32 texts per call, so a queued /classify waits < 10 s | contract constant `EMBED_BATCH` |
| Privacy | 0 bytes of block text leave the machine: the helper listens on 127.0.0.1 only and the model runs locally | helper bind address in verdict_server.py |
| Helper memory | ≤ 1.5 GB extra RSS once the model is loaded | measured 1.19 GB in the trial |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised (stopword-only ask, mid-way dismiss failure, helper down)
- [ ] Ask quality and noise precision in §5 are re-measured on the live DB after install
- [ ] The Owner sees the noise tag and the Topics section working in Chrome on real data (CHK)
- [ ] Test seam(s): Rust unit tests over an in-memory DB with a mock /embed server — catches: ranking, vectors cache, clustering, report shape, degrade path / misses: real model quality. Web `bun test` component tests — catches: rendering and click flows / misses: real daemon data.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | The Owner pre-approved this spec and the stats list (instruction: "assume I approve of the spec") | High | the whole plan |
| A2 | The noise threshold is ≥2 past dismissals with no other label (measured 97.7% / 78.1%) | High | a few wrong tags; dismiss still needs a click |
| A3 | Topic count k = clamp(round(√(n/2)), 2, 8) is readable; the trial's silhouette pick sat at the range edge | Med | topics too coarse or too fine; the constant is in the contract |
| A4 | Block text = description + up to 5 of its prompts (300 chars each), with Jira keys stripped | Med | weaker topics on weeks without prompts (before 2026-10-05) |
| A5 | Embeddings run inside the existing classifier helper, so the off switch, supervisor and uv env are reused | High | a second helper process would be needed |
| A6 | Loading the model lazily, on the first /embed call, keeps helper start-up as fast as today | High | slow helper starts |
| A7 | Theme map evidence is weak (embedding AMI only +0.03–0.07 over TF-IDF); display-only makes that acceptable | Med | topics look arbitrary; nothing is corrupted |

## 8. Open questions

None. Every open fork above is recorded as an Assumption under the Owner's pre-approval.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Helper | The local Python classifier server on 127.0.0.1:9324, supervised by the daemon |
| Topic | A group of work blocks whose texts are alike, found by clustering their vectors |
| Switch | Two consecutive work blocks in one day that sit in different topics |
| Spread | 1 − the mean pairwise similarity of one ticket's blocks |
| Noise host | A browser host whose every earlier labelled event was dismissed, at least twice |
