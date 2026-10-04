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
5. Report the new key and link. Create already moved it to In Progress; if that move failed, the error names the key — never create again.

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
