# Spec: Google Calendar 3LO login

**Created**: 2026-10-08 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Every Google Calendar error tells the Owner to run `worklog collect gcal --auth`, but that command does not exist (it was lost in the Python → Rust port). Nothing can create the first token file, so Calendar collection is dead for anyone without a token left over from the Python era.

**Solution**: `worklog collect gcal --auth` opens the browser, the Owner clicks "Allow" on Google's consent page, and worklog saves a token the existing collector already knows how to refresh.

**Who it's for**: The Owner — the one person running worklog on their own machine.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: The OAuth client (id + secret) is the Owner's own, downloaded from Google Cloud Console as a "Desktop app" credential; worklog ships no built-in client.

## 1. Context

### 1.1 Problem statement

The Calendar collector can refresh an existing token but cannot obtain one. Seven error messages and the `worklog day` empty-day diagnostic all point at an `--auth` flag that the command line rejects. The Owner has no token on disk today, so no meeting ever lands on the timeline.

**Current workaround**: None that works. Producing the token file by hand means running Google's own OAuth tooling outside worklog and reshaping its output.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Runs worklog locally, owns the Google account and the OAuth client | Single user, comfortable with a terminal, has a browser on the same machine |

**Primary actor**: Owner.
**Hidden stakeholders**: The Calendar collector (consumes the token file this writes); `worklog day`'s empty-day diagnostic (already points at this command).

## 2. Scope

### 2.1 In scope

- A login command that runs Google's authorization-code flow for an installed app, with a one-time local listener catching the redirect.
- PKCE and a random `state` value on every login.
- Writing the token file in the exact shape the existing refresh code reads.
- Clear, one-line failures for every way the login can go wrong, with the consent link printed so the Owner can open it by hand.
- README steps for creating the Desktop-app credential in Google Cloud Console.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No "Sign in with Google" button in the web review UI (spec 001 already ruled it out).
- No write access — the requested scope stays read-only calendar.
- No Gmail, Drive or other Google products.
- Not fixing whether the setup wizard's pasted `google_refresh_token` reaches the collector — filed separately as an issue.
- No built-in, worklog-owned OAuth client; no device-code flow for headless boxes.

## 3. Journeys

### Journey 1 — First Google login (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A valid Desktop-app credentials file is in the config dir, no token file | Owner runs the login command and clicks "Allow" | The terminal says Calendar is connected; the token file exists with owner-only permissions; the next collect run pulls events |
| Error | Credentials file is missing | Owner runs the login command | One line naming the missing file and the Google Cloud Console step; non-zero exit; nothing written |
| Error | Owner clicks "Deny" on the consent page | The browser lands on the redirect | The browser tab says login was cancelled; the terminal says the same; non-zero exit; any existing token file is unchanged |
| Edge | No browser can be opened (headless or opener missing) | Owner runs the login command | The consent link is printed; the Owner opens it by hand on the same machine; the flow completes as in Happy |
| Edge | Owner never finishes the consent page | 5 minutes pass | The command stops with a timeout message and non-zero exit; the listener is closed; existing token unchanged |

### Journey 2 — Re-login after a revoked token (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A token file exists but its refresh token was revoked | Owner runs the login command and clicks "Allow" | The token file is replaced with the new one |
| Error | The redirect carries a `state` that does not match | The listener receives it | The login stops at once (no further waiting); the browser tab and terminal both report a possible forged redirect; non-zero exit; token unchanged |
| Edge | Google returns no refresh token (Owner already granted consent before) | Code exchange succeeds without one | The command fails, telling the Owner to remove worklog's access at myaccount.google.com and retry; token unchanged |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST be able to start the login with `worklog collect gcal --auth` | CLI test: the flag parses and reaches the login routine |
| FR-02 | MUST | The login MUST read `installed.client_id` and `installed.client_secret` from `google_credentials.json` in worklog's config dir (`~/.config/worklog/`, or `$WORKLOG_HOME` when set) | Unit test with a fixture credentials file |
| FR-03 | MUST | The login MUST fail with a message naming the credentials file when it is missing or unparsable | Unit test asserts the message contains the path |
| FR-04 | MUST | The login MUST request only the scope `https://www.googleapis.com/auth/calendar.readonly`, with offline access and forced consent | Unit test inspects the built consent URL |
| FR-05 | MUST | The login MUST send a PKCE challenge and verify it at code exchange | Unit test: mock token endpoint matches on the verifier whose hash is in the consent URL |
| FR-06 | MUST | The login MUST stop with an error, without waiting for another redirect, when the redirect's `state` differs from the one it sent | Unit test with a forged state: command fails, token unchanged |
| FR-07 | MUST | The login MUST print the consent link every time, and also try to open it in the browser | Unit test with a fake opener returning "unsupported" still prints the link |
| FR-08 | MUST | The login MUST write `google_token.json` in the same config dir as a JSON object with fields `token`, `refresh_token`, `token_uri`, `client_id`, `client_secret`, `scopes` (array of strings) and `expiry` (RFC 3339, UTC) | Round-trip test: written file is accepted by the existing refresh routine without a refresh call |
| FR-09 | MUST | The token file MUST be readable only by its owner | Unit test checks mode 0600 on unix |
| FR-10 | MUST | A failed login MUST leave any existing token file byte-for-byte unchanged | Unit test: pre-existing file, denied consent, file unchanged |
| FR-11 | MUST | The login MUST stop waiting after 5 minutes with a timeout message | Unit test with a shortened, injected timeout |
| FR-12 | MUST | Owner MUST see a cancellation message in both browser and terminal when consent is denied | Unit test: `error=access_denied` redirect |
| FR-13 | MUST | The login MUST fail when the token response has no refresh token, telling the Owner how to reset consent | Unit test with a mock response lacking `refresh_token` |
| FR-14 | MUST | `worklog collect gcal --auth` MUST be accepted by the command line; `--auth` with any other collect target MUST be rejected with a message naming `gcal` | CLI tests: gcal+--auth with no credentials file fails with the FR-03 message (not a parse error); jira+--auth fails naming gcal |
| FR-15 | MUST | The login MUST fail with a one-line message carrying the HTTP status when the token endpoint is unreachable or returns an error | Unit test with a mock 400 response; token unchanged |
| FR-16 | MUST | The login MUST fail with a one-line message when it cannot open the local listener | Covered by code review; no test (cannot force a loopback bind failure portably) |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Consent wait limit | 300 s, then stop | Timeout constant + injected-timeout test |
| Listener exposure | Bound to 127.0.0.1 only; the first redirect carrying `code`, `error` or `state` ends the wait; closed after | Unit test asserts the bound address is loopback |
| Token file permissions | 0600 | Unit test on unix |
| Owner time, happy path | < 60 s from command to "connected" | CHK001 human run |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test under `cargo test --manifest-path rust/Cargo.toml`
- [ ] The Deny, timeout, missing-credentials and forged-state paths are each exercised by a test
- [ ] clippy and fmt gates are clean
- [ ] The Owner runs the real login once, then `worklog collect gcal`, and sees today's meetings land (CHK001)

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | The Owner creates their own "Desktop app" OAuth client and saves its JSON as `google_credentials.json` | High | Login can't start; README step covers it |
| A2 | Google accepts a loopback redirect on any port for Desktop-app clients | High | Would need a fixed port; small change in one module |
| A3 | The credentials JSON uses the `installed` top-level key (Desktop type) | High | Parse fails with a clear message; accepting `web` too is a one-line follow-up |
| A4 | The browser runs on the same machine as worklog | High | SSH users must port-forward; device flow is a non-goal |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| 3LO | Three-legged OAuth: the Owner approves access in the browser and the app receives a token |
| PKCE | Proof Key for Code Exchange: a one-time secret that makes a stolen authorization code useless |
| Loopback redirect | Google sends the browser back to `http://127.0.0.1:<port>`, where worklog listens once |
| Credentials file | `google_credentials.json`, the Owner's OAuth client from Google Cloud Console |
| Token file | `google_token.json`, the access + refresh token the collector uses |
