# CHK003 — PREP Verify on 2026-09-25 (2026-09-27)

Same private setup as CHK002 (backup copy of the live DB, branch daemon :9433, `next dev -p 3334`).

1. No personal-account commit in any work block
   - `SELECT count(*) FROM events WHERE repo LIKE 'TomasPalsson/%'` → 0
   - personal-repo github rows linked into any block → 0 (see also CHK001-phase-a.md).
2. vitinn-infra Details view shows every prompt, tool call and shell command in its span
   - see CHK002-details.md: block 5282 view == DB (5 prompts, 115 tool calls, 66 helper rows,
     0 shell in span).
3. The vitinn-infra billing line (Sjúkra, 2 h) — generated via `POST /billing/lines/regenerate`
   (real `claude -p`, D-02 input only):
   > Í dag var unnið að lagfæringum fyrir Sjúkra kerfið. Litavilla í viðmótinu var löguð og
   > stillingar fyrir gervigreindarlíkön voru uppfærðar og betrumbættar. Einnig var leyst úr villu
   > sem hafði áhrif á sjálfvirka keyrslu í bakendakerfinu.
   3 Icelandic sentences, no numbers, ticket/PR numbers, paths or tool names; passed `validate`.
   The APRÓ/LibreChat line (6 h) likewise: 3 Icelandic sentences.

Found and fixed during this check:
- The first real run stored `{"text": "..."}` — the model nested the reply inside `text`.
  Fixed (unwrap + validator rejects braces), with two regression tests.
- Haiku's Icelandic was weak (a garbled word, "tók tvo tíma"). Line texts now use
  `LINE_TEXT_MODEL = claude-sonnet-5` and the prompt forbids mentioning time spent.
- Badge/Edit/Regenerate sat inside the 2-line clamp and were cut off on long texts; moved to their
  own row, styled, clamp raised to 3 lines (a ≤400-char text fits whole). Screenshot:
  CHK003-billing-lines.png.

Also exercised on real data:
- Edit → textarea with the current text; Cancel returns; the card does not collapse.
- Manual text + Regenerate → `{"generated":false,"reason":"hand-edited"}`, text unchanged (B10).
- Empty manual text → row deleted → export shows the old joined text with text_origin null,
  i.e. the "not generated" badge (FR-35).

Owner judgement ("would send to their boss unedited") is the owner's call; this run was ticked
under the standing approval the owner gave before leaving.
