# Spec: Customer-safe Tempo descriptions

**Created**: 2026-10-20 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Generated Tempo line texts go to customers on their invoice, and in the 20 Sep – 19 Oct review 13 of 18 needed fixes were worklog-written lines: texts that talk about the evidence ("Engar upplýsingar liggja fyrir…", "vísbendingarnar staðfesta ekki…"), spell versions as words ("five point five"), name colleagues and Slack, mix three customers in one line, or use a plural verb ("Mældu"). Nothing caught them: the Verdict check ran on 7 of 94 lines, and meeting and hand-typed lines are never checked.

**Solution**: The writer is told the four missing rules, and a deterministic validator rejects the mechanical failures so the existing re-ask loop rewrites them; meeting and hand-typed lines get the same validator as a "needs a look" flag, never a rewrite.

**Who it's for**: The developer who reviews and sends their own Tempo lines (Tómas), and the customers who read those lines on invoices.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: The checks are plain rules in code, not a model call, so they run on every line every time; what a rule cannot see (spelling like "Metnaði", internal hostnames like "apro-prod") is left to the prompt.

## 1. Context

### 1.1 Problem statement

A generated line passes `line_text::validate` today if it has 1–3 sentences, no path, no tool name and one Icelandic letter (observed: `rust/crates/worklog-core/src/line_text.rs:37`). The prompt repeats "only what the clues show" (`line_text_prompt.rs:13-41`), which the model turns into sentences about the clues. The Verdict check (`line_check.rs:17-27`) returns "not checked" whenever Verdict errors or the ticket has no summary, and meeting lines skip it (`daemon_tempo_lines.rs:166`).

**Current workaround**: A manual review of every Tempo entry after the fact, then hand edits in Tempo.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Developer | Generates, reviews and sends Tempo lines in the review UI | Trusts a line that has no "needs a look" flag |
| Customer | Reads Tempo descriptions on the invoice | Sees exactly the text, no context |

**Primary actor**: Developer.
**Hidden stakeholders**: Billing (APRÓ invoicing) reads the same lines.

## 2. Scope

### 2.1 In scope

- Four new rules in the Icelandic system prompt
- Deterministic validator rules that reject evidence talk, "Slack", spelled-out numbers, a plural first verb, and foreign customers
- The same rules as a "needs a look" flag on meeting lines and hand-typed lines, plus a flag when the text is just the ticket summary

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No checking of entries typed directly in Tempo (owner "outside").
- No change to how a block's ticket is picked (wrong-ticket cases such as GENAI-1987 / GENAI-1917).
- No new model call for grammar or spelling; "Metnaði"-type spelling slips are not caught.

### 2.3 Ask first

- new runtime dependency
- CI/workflow config
- schema migration

## 3. Journeys

### Journey 1 — A generated line hedges (Developer)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | the model writes "…en vísbendingarnar staðfesta ekki hvort það lauk." | the line is generated | the text is rejected with a reason and re-asked; the stored text has no evidence talk |
| Error | all 3 attempts break a rule | the line is generated | today's behaviour: an explicit Generate fails loudly, the automatic pass leaves the line without text |
| Edge | a good text such as "Bætti Claude Haiku 5.5 við model-stillingar MMS-tenantins…" | the line is generated | it passes unchanged on the first attempt |

### Journey 2 — A hand-typed line repeats the ticket title (Developer)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | ticket summary "Hertz - Greiningarfundur" | the developer saves the text "Hertz - Greiningarfundur" | the text is saved as typed and the line shows "needs a look" |
| Error | the ticket has no stored summary | the developer saves any text | the text is saved; only the text-only rules can flag it |
| Edge | the developer saves a text that passes every rule | — | it is saved with no flag, exactly as typed |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The validator MUST accept, unchanged, every good text in Appendix B | unit test over Appendix B |
| FR-02 | MUST | The validator MUST reject a text that contains any evidence-talk phrase listed in Appendix A | unit test: B-bad 1, 5, 6 rejected with a reason naming the rule |
| FR-03 | MUST | The validator MUST reject a version number written in words (a number word next to "point" or "komma", e.g. "five point five"); number words in ordinary prose ("sex mánaða", "einum hópi") are allowed | unit test: B-bad 3 rejected |
| FR-04 | MUST | The validator MUST reject a text that names Slack | unit test: B-bad 4 rejected |
| FR-05 | MUST | The validator MUST reject a text whose first word is a plural past verb (ends in "u") | unit test: B-bad 4 rejected for its first word |
| FR-06 | MUST | A ticket line MUST be rejected when its text names a known customer other than the ticket's, or two or more known customers when the ticket's customer is unknown; APRÓ never counts; matching follows design.md §7 | unit test: B-bad 2 on a Sjúkra ticket rejected; a VÍS text on a VÍS ticket accepted |
| FR-07 | MUST | The Icelandic system prompt MUST tell the writer: never describe the clues or what is unknown; never name colleagues, Slack or internal channels; write version numbers as digits; describe only the ticket's own customer | unit test asserts each rule's phrase is in the prompt |
| FR-08 | MUST | A meeting line or hand-typed line that breaks a text rule MUST be flagged "needs a look" and keep its text exactly | test: manual save of B-bad 1 keeps the text and sets the flag |
| FR-09 | MUST | A meeting line or hand-typed line whose text equals its ticket summary (ignoring case and spaces) MUST be flagged "needs a look" | test: "Hertz - Greiningarfundur" on that ticket is flagged |
| FR-10 | SHOULD | The rejection reason fed back to the model SHOULD name the broken rule in Icelandic (new rules only; existing reasons unchanged) | unit test on the re-ask message |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Extra model calls per line | 0 beyond today's max of 3 attempts + 1 Verdict regenerate | code review of the write loop |
| Validator time | under 5 ms per line for a 600-character text | unit test timing over Appendix B |
| Customer leakage | 0 of the Appendix B bad texts naming a foreign customer get past the writer | FR-06 test |
| Registry unreadable | the customer rule is skipped, every other rule still runs, the line is still written | unit test with no `known_customers` key |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of Journey 1 keeps today's behaviour (existing tempo_line_writer tests green)
- [ ] External check: `cargo clippy --all-targets --all-features -- -D warnings` — type and lint errors the suite cannot argue with
- [ ] Test seam(s): `line_text::validate` and the ticket-line validator as pure functions — catches: every rule on real texts / misses: whether the model obeys the new prompt rules

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | "five point five" came from the "never mention hours" rule colliding with version "5.5" | Med | the prompt rule alone may not stop it; FR-03 still rejects it |
| A2 | First-person past verbs never end in "u" in these texts | High | a good line is re-asked once |
| A3 | A text naming Slack is never wanted on an invoice, even "svar fyrir Slack" (GENAI-1953, 7 Oct) | Med | such lines get rewritten without the word |
| A4 | B-bad 2's customers resolve through the billing customer registry aliases (VIS→VÍS, Byko→BYKO, Sjúkra) | High | FR-06 misses mixed-customer lines |
| A6 | Hand-typed and meeting lines are checked with the customer-safe rules and the summary-copy rule only, not the full generated-text validator (they are often one sentence) | High | one-sentence hand texts would all be flagged |
| A5 | Of the 13 worklog-written fixes, the rules catch 7 (Appendix B bad set); F12 "apro-prod", F30 "Metnaði", F31 "Rakst í VIS", F34 filler rely on the prompt; F02/F03 are ticket choice (non-goal) | High | those four can recur |

## 8. Open questions

None identified.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Line text | The Tempo description for one ticket on one day (`tempo_line_texts`) |
| Evidence talk | A text containing, case-insensitively as a substring, one of: "liggja fyrir", "liggur fyrir", "vísbending" (covers every inflection), "staðfesta ekki", "staðfestir ekki", "ekki staðfest", "óljóst hvort", "ekki ljóst hvort" |
| Known customer | A row in the billing customer registry, matched by name or alias |

## Appendix B — Real texts from 20 Sep – 19 Oct

The `(Customer)` suffix after a good text is the ticket's customer, not part of the text.

**Good (must pass):**
1. Tengdi VÍS Geitin frá Boost.ai við LibreChat í VÍS-tenantnum í gegnum LiteLLM og lagaði A2A-skilaboð þannig að agentinn fái nýjustu notandaskilaboðin; takmarkaði jafnframt aðgang með netfangi og setti upp samhengi fyrir hvert spjall. Undirbjó uppfærslu LiteLLM í v1.104.0 fyrir apro-prod. (VÍS)
2. Vann að svari fyrir SJTR um logging storage í APRÓ AI. Tók mið af spurningum um hvaða upplýsingar skrást þegar starfsfólk notar Run Code og hvort hægt sé að fá afrit af loggunum. (Sjúkra)
3. Endurskoðaði umfangsmat og kostnaðartillögu fyrir LiteLLM-uppsetningu RL fyrir marga sjóði: bætti við breytilegum tíma fyrir fundi og kynningar, hækkaði Enterprise-kostnaðinn og skýrði að mögulegur Keydesk-kostnaður væri óákveðinn. Kannaði í LibreChat hvort hægt sé að takmarka sýnileika módela við ákveðna user groups og hvort notendur geti tilheyrt fleiri en einum hópi. (RL)
4. Bætti Claude Haiku 5.5 við model-stillingar MMS-tenantins, samhliða Sonnet og Opus 5.5. Þetta gerir Haiku 5.5 aðgengilegt í þeirri tenant-uppsetningu. (MMS)
5. Útbjó aggregation fyrir Matís til að laga custom agentinn í production, svo hægt sé að keyra uppfærsluna á Matís-gagnagrunninum. Skoðaði líka hvernig agentinn fær tools og hvort hann geti notað bæði innbyggða SearchNexNG-vefleit og Anthropic-vefleit. (Matís)
6. Setti upp og virkjaði Data retention fyrir Sjúkra með sex mánaða varðveislu samtalssögu. Staðfesti að breytingarnar væru komnar í main og lokaði worktree-inu. (Sjúkra)
7. Uppfærði LibreChat-stillingu Hertz-tenantins í production þannig að hægt sé að hengja allt að 50 skjöl við eitt spjall. Þetta gerir Hertz-notendum kleift að senda fleiri skjöl í einu spjalli. (Hertz)
8. Lagaði í LibreChat að inline-tenglar á skrár sem code interpreterinn býr til opnist sem 404 í stað þess að sækja viðhengi úr spjallinu; tenglar án samsvarandi viðhengis birtast nú sem venjulegur texti. Bætti við prófunum, backporta uppfærsluna í LibreChat 0.8.8 og byggði nýja LibreChat-mynd. (Sjúkra)

**Bad (must be rejected):**
1. Vann að samráði við Coripharma um Code Interpreter á APRÓ AI platform, með það að markmiði að fara yfir stöðu, opnar spurningar og næstu skref. Engar upplýsingar liggja fyrir um að fundurinn hafi verið haldinn eða hvaða niðurstöður fengust. — evidence talk
2. Rannsakaði 503-villur sem komu upp hjá Sjúkra í production þegar Code Interpreter var notaður. Reiknaði Lambda-kostnað eftir notkun á hvern virkan notanda út frá raungögnum frá Vis, Byko og Sjúkra. — foreign customers (Sjúkra ticket)
3. Bætti Claude Sonnet five point five við Claude Desktop profile Sjúkra og stillti LiteLLM í production til að bjóða líkanið þar. Þannig geta notendur Sjúkra valið líkanið í Claude Desktop. — spelled number
4. Mældu aukinn biðtíma þar til Gemini 3.8 byrjar að svara og skoðaði hvað gæti valdið hægari svörum. Sendi Hera tillögur að úrbótum í Slack og bað um álit hennar til að samræma næstu skref. — plural verb, Slack
5. Rannsakaði hvers vegna code interpreterinn í Vitinn hjá Sjúkra skilar skráarslóðum í stað niðurhalanlegra viðhengja í spjallinu. Skoðaði nýlega LibreChat-uppfærslu og stillingar til að afmarka orsökina; engin breyting eða staðfest lausn liggur fyrir í vísbendingunum. — evidence talk
6. Yfirfór hvernig setja á upp shareable conversation links í LibreChat þannig að innskráðir notendur geti deilt samtölum en gestir sem ekki eru innskráðir geti ekki opnað hlekkina. Fylgdist með rolloutinu á apro-prod eftir samþykki, en vísbendingarnar staðfesta ekki hvort það lauk. — evidence talk
7. "Hertz - Greiningarfundur" saved by hand on ticket "Hertz - Greiningarfundur" — equals summary (FR-09)
