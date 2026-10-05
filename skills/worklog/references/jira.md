# Jira assistant (Recipe J)

Verbs: `worklog ticket get|start|find|move|create|hints` and `worklog account allowed|suggest|relearn`. All accept `--json`. Only GENAI tickets are touched; others are left alone.

## Routing

| User says | Do |
|-----------|----|
| work on KEY | `worklog ticket start KEY` (To Do to In Progress). No confirm. |
| work on free text | `worklog ticket find "<text>"`. Matches exist: offer them and start the pick. None fit: create (below). |
| waiting on something | Ask "Move KEY to Blocked?", then `worklog ticket move KEY Blocked`. |
| finished / PR merged | Ask "Move KEY to Done?", then `worklog ticket move KEY Done`. |
| what can I close? | `worklog ticket hints` and list the suggested moves; each move still asks. |

Never post to Slack or any chat tool; the only outputs are Jira and the terminal.

## Creating a ticket

1. `worklog account suggest "<request text>"` ranks accounts; `worklog account allowed` lists the valid ids.
2. Draft the summary and description (text guide below) and write the description to a temp file.
3. One confirm, showing the title and the account. Nothing else is asked.
4. `worklog ticket create --summary "<title>" --description-file <file> --account <id>`. When suggestions were shown, add `--guessed <first-suggestion-id>` (even if the user corrected it — that is how a wrong guess is learned), and `--clue "<phrase>"` (repeatable) for words from the request that point to the account.
   The ticket is assigned to the user by default; never ask who it is for. Only when the user says otherwise: `--assignee <jira-accountId>` for someone else (ask for the accountId if it is not known), or `--unassigned` to leave it empty.
5. Report the new key, link and assignee. Create already moved it to In Progress; if that move failed, the error names the key — never create again.

## This session's ticket

The SessionStart hint says which ticket this session is on.

1. Hint names a key: nothing to do.
2. No key: once the task is clear, run `worklog ticket find` (try two phrasings).
3. Evidence order: an exact key in the prompt, branch, folder or a PR beats any search result. Never replace a key the Owner named with a similar-looking result. Fuzzy matches are weak.
4. One clear match: use it.
5. Otherwise ask the Owner ONCE: their key, or create (steps above).
6. Before creating, a same-problem hit is the ticket (duplicate check). Recurring or templated tickets are distinct per instance. A bug and a feature are never duplicates.
7. After create, read back with `worklog ticket get KEY` and report key + URL.
8. Then `worklog ticket use KEY --session <id>` (id from the hint). No confirm; it only writes locally.

Nobody present: record nothing, create nothing.

## When worklog's verbs aren't enough (twg CLI)

Only if `twg` is installed. Read-only.

```bash
# my in-progress tickets
twg jira workitem query --jql 'assignee = currentUser() AND statusCategory = "In Progress" ORDER BY updated DESC' --fields summary,status
# full context
twg context jira workitem KEY
# PRs linked to a ticket
twg context jira workitem KEY --types ExternalPullRequest
```

- Never run `twg login/setup/auth` unless asked.
- Never guess required fields or IDs; ask.
- Create GENAI tickets only with `worklog ticket create` (it learns accounts), never `twg jira workitem create`.
- NEVER log time with `twg jira workitem worklog add`. worklog syncs Tempo itself; that would double-log.

## Text guide

Write in English with no emoji. Describe the work in your own words; never paste the user's chat. Length scales with the task.

- One short opening paragraph: what and why.
- Then only the sections that have content:
  - `## Tasks` as checkboxes: `- [ ] Register the app in the Entra tenant`
  - `## Done when` as plain `-` bullets of observable outcomes
  - optional context or links
- No empty sections and no fixed template.

Example:

```markdown
Add Microsoft (Entra ID) single sign-on to the Innnes portal so staff log in with their work accounts instead of local passwords.

## Tasks
- [ ] Register the app in Innnes's Entra tenant
- [ ] Add OIDC login to the portal
- [ ] Test with two Innnes test users

## Done when
- Staff log in with their work account
- Local passwords are no longer needed
```
