# Changelog

## [0.6.0](https://github.com/TomasPalsson/worklog/compare/v0.5.0...v0.6.0) (2026-04-19)


### Features

* events submenu + ticket-flow fix + design polish + upgrade daemon-restart (v0.6.0) ([#5](https://github.com/TomasPalsson/worklog/issues/5)) ([a53f493](https://github.com/TomasPalsson/worklog/commit/a53f493e2caec647b892857fbfeeab230344cbf7))

## [0.5.0](https://github.com/TomasPalsson/worklog/compare/v0.4.0...v0.5.0) (2026-04-19)


### ⚠ BREAKING CHANGES

* remove Claude hook; add `worklog setup` wizard
* drop companies; Claude picks Jira ticket; cache open tickets

### Features

* CLI polish + web auto-fetch + release-please automation ([0fb6ac9](https://github.com/TomasPalsson/worklog/commit/0fb6ac9eee5f8443e292d6348326b7731299e29f))
* **cli:** GREEN — fix help/version exit codes + add serve/upgrade aliases ([bdd092c](https://github.com/TomasPalsson/worklog/commit/bdd092ceb6404f3a5b5e066b7abd24302b7a20c9))
* **cli:** polish output with console + indicatif ([92a4916](https://github.com/TomasPalsson/worklog/commit/92a491672cf543d70979a7cf244d910f4255849e))
* **cli:** worklog doctor + Rust hook preference in hook install [GREEN] ([5021291](https://github.com/TomasPalsson/worklog/commit/5021291e3ccd1f221170ba8dd1af656a9dff0e4f))
* daemon auto-install + CLI polish + rolling purge + richer hook capture + web dark mode (v0.4.0) ([#4](https://github.com/TomasPalsson/worklog/issues/4)) ([0a2a513](https://github.com/TomasPalsson/worklog/commit/0a2a513968991fe90dc042dfb15275df440663e7))
* **daemon:** TCP listener alongside unix socket (stage 4.3) ([310b233](https://github.com/TomasPalsson/worklog/commit/310b2339b1b1e401ec2335b0a4de47828638dc0c))
* **day:** GREEN — 'worklog day' orchestrator in Rust ([37b65a4](https://github.com/TomasPalsson/worklog/commit/37b65a4ed3a88090ae9c25caeeeb71f807603658))
* **db:** schema v2 with sessions, blocks, session_id [GREEN] ([e040f56](https://github.com/TomasPalsson/worklog/commit/e040f565b280be2b0994e8da5e572d20060e321d))
* delegate hook to Rust + add schedule passthrough (stage 1.2 close) ([9b6b1d9](https://github.com/TomasPalsson/worklog/commit/9b6b1d99eeab280540b0fe73f1c9e694be2d2197))
* delete the Python package, ship pure-Rust via signed curl installer ([b596bea](https://github.com/TomasPalsson/worklog/commit/b596bea2351e4634d2e0e02fcee95619f2fe1c2a))
* drop companies; Claude picks Jira ticket; cache open tickets ([101aaf5](https://github.com/TomasPalsson/worklog/commit/101aaf5daee8835c30d6da07daaaafdd95d9b378))
* **estimate:** claude -p block estimator + CLI [GREEN] ([746a691](https://github.com/TomasPalsson/worklog/commit/746a691fa6b314d1bb3fb7c7b593970a7bacd1cd))
* **gcal:** GREEN — implement Rust Gcal collector ([7d2597d](https://github.com/TomasPalsson/worklog/commit/7d2597dfc448600a7c424da577e1953b9a1f2410))
* **infer:** gap-timeout block clustering + CLI [GREEN] ([a84535a](https://github.com/TomasPalsson/worklog/commit/a84535ae49d07c714a945f007ca1c27d527ea9d2))
* **install:** curl-piped installer + smoke tests ([0835568](https://github.com/TomasPalsson/worklog/commit/0835568b4b1cff0f80367230536718a014ecd3f2))
* migrate Python CLI to delegate to Rust binary ([971a23b](https://github.com/TomasPalsson/worklog/commit/971a23b72357fb93b0000bae16944f6c100a624e))
* **release:** GREEN — real Ed25519 pubkey + GHA release workflow ([2b80ed5](https://github.com/TomasPalsson/worklog/commit/2b80ed585ed666791222db6e87f870a834ff3cc8))
* remove Claude hook; add `worklog setup` wizard ([a44bf4d](https://github.com/TomasPalsson/worklog/commit/a44bf4d91e5d1ab9c9e45eccc3a3c17d45670885))
* **rust-hook:** working worklog-hook binary [GREEN] ([4ea8546](https://github.com/TomasPalsson/worklog/commit/4ea854612b436f29122943dc5bc4b91d38ed5b5c))
* **rust:** axum unix-socket daemon + block service (stage 3.2 close) ([3a8b3fa](https://github.com/TomasPalsson/worklog/commit/3a8b3fa23432d68fd3cf40d828552aa4f8a715b9))
* **rust:** hook + schedule modules + wizard integration (stage 1.2) ([2493dcc](https://github.com/TomasPalsson/worklog/commit/2493dccfae2aa48e168993e88c36d62be562456c))
* **rust:** infer + hook-run + estimator modules (stage 3.1) ([b8e77e8](https://github.com/TomasPalsson/worklog/commit/b8e77e84386e72bb2de7815d358be150476524b1))
* **rust:** jira, github, tempo collectors + HTTP client (stage 2.1) ([552c41c](https://github.com/TomasPalsson/worklog/commit/552c41c541ff17b57884c44486e0e028c1b7013f))
* **rust:** scaffold worklog-core + worklog-cli (stage 1.1) ([5954043](https://github.com/TomasPalsson/worklog/commit/59540436dce44d6da25e5f6c74b4e48f3662b9d4))
* **rust:** setup wizard, CI, Justfile, crate README (stage 1 close) ([d33a84f](https://github.com/TomasPalsson/worklog/commit/d33a84f9972aa7e9407f88a0bdd4096c2f0dc8c2))
* **rust:** worklog collect + sync CLI + Python delegation (stage 2 close) ([a844f18](https://github.com/TomasPalsson/worklog/commit/a844f1840e8b3d95404b55d793e980bfa3d30068))
* **sessions,hook:** session pairing + reaper wired into Claude hook [GREEN] ([899b3ef](https://github.com/TomasPalsson/worklog/commit/899b3ef26cf378e7d776a41f3b9f7bf12463c65a))
* **setup:** restore Claude hook + wire install prompt into wizard ([bec4c6f](https://github.com/TomasPalsson/worklog/commit/bec4c6fb1fb5b31e511d24e176414a30d9ccc9de))
* **tempo,web:** sync + UI operate on blocks [GREEN] ([08877e3](https://github.com/TomasPalsson/worklog/commit/08877e3612e8d232ca2f82fde7e6809298e0979f))
* **updater:** crypto + manifest + delta + install scaffolding (stage 5.1) ([9dbefd7](https://github.com/TomasPalsson/worklog/commit/9dbefd705f759554fc9f0c97a20ddac18c15937c))
* **updater:** self-update CLI, dev tooling, python upgrade routing (stage 5 close) ([2d29d23](https://github.com/TomasPalsson/worklog/commit/2d29d236ce438b498d96c0ddc46a57eae25687e9))
* **web:** auto-fetch web/ tree from GitHub archive ([4ee1d12](https://github.com/TomasPalsson/worklog/commit/4ee1d128ef2a8843373e35c8061e2d719e745923))
* **web:** Next.js + Bun app + daemon /estimate & /sync (stage 4.1) ([07f10aa](https://github.com/TomasPalsson/worklog/commit/07f10aa083888570eaa2a591680281f98a118294))
* **web:** readable card layout with icons ([7c3c6b6](https://github.com/TomasPalsson/worklog/commit/7c3c6b65bbc14b7f20907ad54bea91df42971663))
* **web:** redesign review UI as log-viewer dashboard ([88c2cf3](https://github.com/TomasPalsson/worklog/commit/88c2cf32b5082f6c32657d2838a3e1a2da36ddc7))
* **web:** searchable ticket combobox + per-block source chips ([49d4df4](https://github.com/TomasPalsson/worklog/commit/49d4df418f65ad37ca744d658df8872e962e5667))
* worklog day (one-shot daily flow) + worklog upgrade (pulls from GitHub) ([e3d01b6](https://github.com/TomasPalsson/worklog/commit/e3d01b604e3a8fabde2d9ec9543401f1dd6d3560))
* worklog web CLI + Dockerfile, retire Python FastAPI (stage 4.2) ([02ab27d](https://github.com/TomasPalsson/worklog/commit/02ab27dc4c9ae44cf8dacda73cb4a7efa6dba16a))


### Bug Fixes

* **estimate:** resilient fallback when claude omits minutes/description ([73786ee](https://github.com/TomasPalsson/worklog/commit/73786eef357fdf9deed61d7de49c6b1f9a3e6dac))
* **hook:** rename install-default to hook-run; add back-compat 'hook run' alias ([551387e](https://github.com/TomasPalsson/worklog/commit/551387e686eb061cd110eb9341b95ead6934ae3a))
* **qa-phase-1:** silent data loss bugs (C1, C8, H1, H2) ([8333b4e](https://github.com/TomasPalsson/worklog/commit/8333b4e4132ae71b205f547484efd7ee24c6eab2))
* **qa-phase-2:** updater correctness (C2, C3, C4, C5, C6, C7, M2, M3) ([6921d88](https://github.com/TomasPalsson/worklog/commit/6921d88369ef5fc979c2a01b4ff24c50f84e8898))
* **qa-phase-3:** boundary + error handling (H5, M1, M4, M5) ([10db970](https://github.com/TomasPalsson/worklog/commit/10db970c7605ff60dd0197010d0e6f47fbaabccc))
* **qa-phase-4:** web UX + accessibility (H6-H12, M6-M8) ([de5c012](https://github.com/TomasPalsson/worklog/commit/de5c012433abc41a3d6765835cf3211d4a1a2fec))
* **qa-phase-5:** timezone correctness (H3, H4) ([7915e8f](https://github.com/TomasPalsson/worklog/commit/7915e8fee5d70fdc54800c20d4b444efd8e00976))
* **qa-round-2:** close findings from second QA wave ([d80ff55](https://github.com/TomasPalsson/worklog/commit/d80ff555e7e8903f11cb8b9b73f168e52819f3b1))
* **qa-round-3:** tempo canary + parity + manifest compat + concurrent safety ([2f0c413](https://github.com/TomasPalsson/worklog/commit/2f0c41383b7e73a0d61224c2a747bb03f593375d))
* **qa-round-4:** delta fallback + pubkey hardening + observability + timeouts ([15fb79c](https://github.com/TomasPalsson/worklog/commit/15fb79c1138d8aa6ee4ea467fbc0fa3dfc6ec6b3))
* **security:** token.json chmod 0600, sanitise OAuth error, always-cleanup key ([56b8958](https://github.com/TomasPalsson/worklog/commit/56b89582e932e418149f7329f0e0f1c2cfe30e87))
* **setup:** generalize token prefix scrubber (GitHub PATs too) ([e728b82](https://github.com/TomasPalsson/worklog/commit/e728b82bb5388dfcaf25aa5fc154489f6f2f3f68))
* **setup:** strip stray glyph before Jira token (Atlassian Copy-button paste artifact) ([aff96cf](https://github.com/TomasPalsson/worklog/commit/aff96cff9c96e8317f31aa22a49d94c3c0aabc80))
* **upgrade:** use SSH to handle private repo auth ([77a6c70](https://github.com/TomasPalsson/worklog/commit/77a6c70b60f1df62676962ff90f015d4c03a54be))


### Documentation

* Docs:  ([7915e8f](https://github.com/TomasPalsson/worklog/commit/7915e8fee5d70fdc54800c20d4b444efd8e00976))
* **qa:** fix doc lies + rot (M10, M12, M13) + clean stale stage refs ([833262d](https://github.com/TomasPalsson/worklog/commit/833262dccd2027b578cfc96b7ae388302bd41e4e))
* rewrite CLAUDE.md + README + migration guide for pure-Rust ([d0bfe9c](https://github.com/TomasPalsson/worklog/commit/d0bfe9ce2bee1ca3466a33a485baed5d3a9b3e4f))
* update READMEs for stage 4 (Next.js + Bun web container) ([27fd010](https://github.com/TomasPalsson/worklog/commit/27fd0101a10577d3728347d428490d8535f49908))
