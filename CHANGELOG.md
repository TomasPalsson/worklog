# Changelog

## [0.13.0](https://github.com/TomasPalsson/worklog/compare/v0.12.0...v0.13.0) (2026-09-26)


### Features

* **005/T001:** schema, seed and multi_tenant flag for tenant folders ([f45ac05](https://github.com/TomasPalsson/worklog/commit/f45ac05a7405906de3d73c80743a9497370bcedc))
* **005/T002:** tenant discovery and links ([f5b179e](https://github.com/TomasPalsson/worklog/commit/f5b179eaff0af433906b68adce4a31ff60872338))
* **005/T003:** clue extraction and the per-block split rules ([539777d](https://github.com/TomasPalsson/worklog/commit/539777d9ab92fe7dbf189f06fdc71aafc0126cf7))
* **005/T004:** hand-set customer shares storage and carving ([90d7225](https://github.com/TomasPalsson/worklog/commit/90d7225d42ccf2b5050e4a7a4d69c29622c11cde))
* **005/T005:** billing export bills each customer its own slice ([5fdacde](https://github.com/TomasPalsson/worklog/commit/5fdacded7be776ac40f4719f3a767ea9617fc4d3))
* **005/T006:** daemon routes for tenants and shares ([09a6efb](https://github.com/TomasPalsson/worklog/commit/09a6efb4c3fb5c37506caa1b1029d61f4243cef9))
* **005/T007:** Billing panel multi-tenant tick and tenant list ([8f01b28](https://github.com/TomasPalsson/worklog/commit/8f01b2860c1c88ffa2bbd5634b7129ab0d2093d9))
* **005/T008:** Block customer split editor (B17) ([111f6a5](https://github.com/TomasPalsson/worklog/commit/111f6a58dbb81414fdf5485cd8b3944c2fe0478c))
* **006/T002:** deild registry CRUD + keyword matching ([1b36ab9](https://github.com/TomasPalsson/worklog/commit/1b36ab9d14712a243e9e80805c00568c63a54982))
* **006/T003:** deild daemon routes ([dc0b670](https://github.com/TomasPalsson/worklog/commit/dc0b67044ed1ff12ed92560cd93a905ee531ac00))
* **006/T004:** deildir editor in Settings -&gt; Billing ([37817cd](https://github.com/TomasPalsson/worklog/commit/37817cd9cf732042b10fd38f5eb6277edfb5dca7))
* **006/T006:** deild resolution ladder + super-block grouping ([c6a27ce](https://github.com/TomasPalsson/worklog/commit/c6a27ceed8281c2b76fba4035b1e939f834ee2af))
* **006/T007:** split routes v2 for every non-personal work block ([f39f95a](https://github.com/TomasPalsson/worklog/commit/f39f95a0b26d62f6dd6ea74f1f828f0c8bef4fde))
* **006/T008:** split editor rows customer + deild + % ([28af5c3](https://github.com/TomasPalsson/worklog/commit/28af5c302f2c998f1768a4e8732dbea717722d5d))
* **006/T009:** move a super block's deild from its line header ([b1d1141](https://github.com/TomasPalsson/worklog/commit/b1d114156016042d628a839e93f574a7fd4891f6))
* **006/T010:** change log — snapshot, diff, feed ([a845235](https://github.com/TomasPalsson/worklog/commit/a8452355919a69ae2a239a6456b3a231b4028111))
* **006/T011:** wire every writer to the change log ([43371e1](https://github.com/TomasPalsson/worklog/commit/43371e1c2fab24ef802495020601e432f146c69c))
* **006/T012:** wire the change-log routes (B11) ([cf1ef07](https://github.com/TomasPalsson/worklog/commit/cf1ef076adfd13ee3d3ae90df4f263a08a6ff1c1))
* **006/T013:** live change pop-ups and catch-up (B10, B11, B15) ([0d77815](https://github.com/TomasPalsson/worklog/commit/0d77815d68828fdcc728e1a2f9730bc4cc7f3867))


### Bug Fixes

* **billing:** a multi-tenant slice's Verkefni only survives for its own customer ([0f80a1a](https://github.com/TomasPalsson/worklog/commit/0f80a1ac2b6759d33026c65dfb5ae95836e5c7d1))
* **billing:** fallback customer slice matches the ticket-summary haystack rows_for_day uses ([a9bbc1c](https://github.com/TomasPalsson/worklog/commit/a9bbc1c199c96bb4709fe683cda41af8cf1c9353))
* **billing:** folderSavePayload no longer drops multi_tenant on save ([b25349a](https://github.com/TomasPalsson/worklog/commit/b25349abcfca45403dd38c5de7a3bb1c93a71699))
* **change-log:** correct notification source and drop duration-driven noise ([419b3a3](https://github.com/TomasPalsson/worklog/commit/419b3a3b9d49caeb47d2be6444c07fe652f5cc01))
* **estimate:** protect hand-set splits from same-ticket merges ([99a39a2](https://github.com/TomasPalsson/worklog/commit/99a39a20fbcff3e1beae8453e2f83dfb7cceb82d))
* **multi-tenant:** billing correctness + split/tenant UI polish ([a468db1](https://github.com/TomasPalsson/worklog/commit/a468db17af9575efe3beec3fcfefec2538e6d292))
* **tenants:** skip hidden directories like .terraform when discovering tenants ([30c67ef](https://github.com/TomasPalsson/worklog/commit/30c67ef6db1bda263878860934f5e4874f3a80b2))
* **web:** a live toast's Show no longer marks the catch-up seen ([5013d06](https://github.com/TomasPalsson/worklog/commit/5013d0668d96e47f231f1c2f1aa463f18e242740))
* **web:** keep the add-customer picker compact and stop split-row styles overriding the registry grid ([2d5b9d5](https://github.com/TomasPalsson/worklog/commit/2d5b9d57325f5eea662aae1e2894164278d11254))

## [0.12.0](https://github.com/TomasPalsson/worklog/compare/v0.11.2...v0.12.0) (2026-09-24)


### Features

* 'What happened' timeline — prompt snippets read live, Claude stretches with branch/tools/files, copied sessions counted once ([be16b84](https://github.com/TomasPalsson/worklog/commit/be16b840dc4244a65f28fa47154e33e28ab2643c))
* **browser-ingest:** heartbeat ingest with work-hours + privacy filters (T002) ([f352b61](https://github.com/TomasPalsson/worklog/commit/f352b61bd2613918af4efdbee3038a8c5bf4ab54))
* **cli:** T004 verdict serve pins git rev + huggingface_hub ([27a12db](https://github.com/TomasPalsson/worklog/commit/27a12dbccb07fc5e30cf1819791aacae7c78aac6))
* **cli:** wire fish shell history and git reflog into `worklog collect` ([c2d396a](https://github.com/TomasPalsson/worklog/commit/c2d396a2a89cf719b40663a8115528eb924ce774))
* **collectors:** add claude_turn transcript collector for the invisible-afternoon gap ([7f7a8f9](https://github.com/TomasPalsson/worklog/commit/7f7a8f9f26f797a0fe4a09ba841501342f2fd586))
* **collectors:** add fish shell history collector ([3f2c386](https://github.com/TomasPalsson/worklog/commit/3f2c38607235de24807d449e2457949e2df34377))
* **collectors:** add git reflog collector ([8793685](https://github.com/TomasPalsson/worklog/commit/87936859bb5fbd75b562656ddd3f199a3848ed31))
* **collectors:** count Claude working in an interactive session, one marker per minute, no text ([16217ac](https://github.com/TomasPalsson/worklog/commit/16217ac9f5ada0488d9f75bcf9a2b11960055544))
* **collectors:** T004 Slack collector for sent messages ([20d4518](https://github.com/TomasPalsson/worklog/commit/20d4518864ab2baa3654ddaef08294e3d567844c))
* **daemon:** answer CORS preflight for extension heartbeats ([69880ae](https://github.com/TomasPalsson/worklog/commit/69880ae53894cdb78632371b15de58cc3060acd6))
* **daemon:** expose overlaps on GET /days/:day and allocation endpoints ([960d0dd](https://github.com/TomasPalsson/worklog/commit/960d0dd0a85a4800045e892a9556246602426692))
* **daemon:** surface block confidence and day gaps on /days/:day ([252c1d5](https://github.com/TomasPalsson/worklog/commit/252c1d5099bd546f82be16f0aa5519189e624842))
* **daemon:** T005 ratio settings (abstain_margin + runner_up_ratio) ([03d9a70](https://github.com/TomasPalsson/worklog/commit/03d9a7020bae7258608a8ebabf5bca6bacd2a7c7))
* **infer:** honour saved overlap allocations when building blocks ([7190ddb](https://github.com/TomasPalsson/worklog/commit/7190ddb0001c261c936b58305033271f40da6897))
* **infer:** parallel projects get parallel blocks instead of one fused block ([9bf3920](https://github.com/TomasPalsson/worklog/commit/9bf39201da305ad7cd0af8a2d37e5f8e0e7766e3))
* **overlaps:** detect overlapping work windows and persist manual splits ([b3bca0f](https://github.com/TomasPalsson/worklog/commit/b3bca0f8a777e969a6ca1633b4c92848699fd5a5))
* **routing:** absorb work-stretch events and hide the rest as noise ([84abefd](https://github.com/TomasPalsson/worklog/commit/84abefd1f9dbed30bbbb9b6050c64c9b27882159))
* **routing:** dismiss events as noise, with optional __ignore__ rule ([1caa291](https://github.com/TomasPalsson/worklog/commit/1caa29192ec1a2dc899ea075a4d7dc4a14e62022))
* **routing:** Link/Context label origins + Slack time-context filing ([a19b7f3](https://github.com/TomasPalsson/worklog/commit/a19b7f351600f060f67b829aefafa8cffe73f8de))
* **routing:** schema v11 and module stubs for browser/Slack event routing ([799fa76](https://github.com/TomasPalsson/worklog/commit/799fa769ca9a7518f2eb0734da2b4d41b56f43d8))
* **routing:** T003 daemon routes for browser/Slack event routing ([ac97b1a](https://github.com/TomasPalsson/worklog/commit/ac97b1a689617256e43339a42df4a0d08c9e7c75))
* **routing:** T003 relative filing rule (abstain margin + runner-up ratio) ([2909ea6](https://github.com/TomasPalsson/worklog/commit/2909ea6f304a9abb528ab420f3693c2ee2ef32c1))
* **routing:** T005 router core — rules, threshold, container narrowing ([fa55128](https://github.com/TomasPalsson/worklog/commit/fa551286789a28113159745d4266977cc6a6889f))
* **routing:** T006 Laya client, helper script and CLI ([19cf90f](https://github.com/TomasPalsson/worklog/commit/19cf90faba6ea6298197cba9b27a820620f562d1))
* **routing:** T007 route before inference in collect all and POST /infer ([acdbeec](https://github.com/TomasPalsson/worklog/commit/acdbeec68ee9cb2241dcdc036056d41b4eaaa574))
* **slack:** title DMs with the counterpart's name via users.info ([4996e31](https://github.com/TomasPalsson/worklog/commit/4996e31dd2ff10871d6e0aedb0c56bd79c36ae70))
* **timeline:** add block_confidence and day_gaps helpers ([5d9eaa6](https://github.com/TomasPalsson/worklog/commit/5d9eaa6b0238564a724abc6ad63f3f654ba73604))
* **upgrade:** bring the web UI and collect agent onto the new binary ([#43](https://github.com/TomasPalsson/worklog/issues/43)) ([d1fc7dc](https://github.com/TomasPalsson/worklog/commit/d1fc7dc7bca94862617c3ea3e3d1695e09ea98b8))
* **verdict:** T002 helper script and client (probability, runner_up, abstain) ([6faf2db](https://github.com/TomasPalsson/worklog/commit/6faf2db53c6a1f7582b7c234b59f276ec7f78f48))
* **web:** calmer day strip — merge same-project stretches, 4-project legend with +N more and Other, labels only when they fit ([546f96c](https://github.com/TomasPalsson/worklog/commit/546f96c2d6785bc233dfe663562613edcf56a19d))
* **web:** click-and-drag time-range selection with saved-allocation brackets ([949d0cd](https://github.com/TomasPalsson/worklog/commit/949d0cd218cd8c9f1c4d83ba47c921667202a45b))
* **web:** day strip groups by the daemon's repo-folded project field ([e5cb1b1](https://github.com/TomasPalsson/worklog/commit/e5cb1b109c4dc644332c0e5b9a432e49a95ab8e9))
* **web:** day strip, grouped sort tray with Not work, compact auto-filed list ([bc8a48e](https://github.com/TomasPalsson/worklog/commit/bc8a48e863ed621fdd9bdc952a0fec47f5377060))
* **web:** drop the no-ticket banner — the owner does not want it ([665f8cb](https://github.com/TomasPalsson/worklog/commit/665f8cb8a15e75a4926022c1192c21f4af8b132d))
* **web:** interactive day strip — lanes view, legend focus, rich tooltip ([536ef44](https://github.com/TomasPalsson/worklog/commit/536ef44871f75b17eb113e3ccb675a84b4789d21))
* **web:** label the day gap that covers 11:30 as lunch ([bba82ea](https://github.com/TomasPalsson/worklog/commit/bba82ea91c20b677a3fe8ca54a817396ba98b02b))
* **web:** linked split sliders that start from today's split, drag measured on the track, 0% shares dropped on save ([40a2b61](https://github.com/TomasPalsson/worklog/commit/40a2b61e4f3d7939d3dad57fd3a7f4267ff86932))
* **web:** overlap types, daemon client and allocation server actions ([8b5c31a](https://github.com/TomasPalsson/worklog/commit/8b5c31ae15ac0e641ff5cce04ef5e760d52f97c2))
* **web:** plain-language origin badges + preview on filed rows ([442a6f7](https://github.com/TomasPalsson/worklog/commit/442a6f70bd3e74e979afb1f96d4d36dcacdb1ff3))
* **web:** render overlap bands, a count chip and the split popover ([fe87073](https://github.com/TomasPalsson/worklog/commit/fe870738dbe71ddea53185daf476c7ccc1894ae1))
* **web:** saved splits read as chips (range, project, percent, reset) under the lanes ([0595637](https://github.com/TomasPalsson/worklog/commit/0595637887c4315cf364dc1bd8ed4f0748943ac6))
* **web:** show block confidence badge and day gaps on the day page ([2969486](https://github.com/TomasPalsson/worklog/commit/2969486e6179829772338db6679242c7e5ce18db))
* **web:** show what each clue said or where it went, and why it was hidden, in the review drawer ([203fe14](https://github.com/TomasPalsson/worklog/commit/203fe1432f5e42d021c6df4492488948528c7a57))
* **web:** T010 routing types, daemon client and server actions ([70fdecf](https://github.com/TomasPalsson/worklog/commit/70fdecfa99031f708a26b120e1362b55a1525d55))
* **web:** T011 unsorted list, label picker with always, source badges (B12) ([3fcdff5](https://github.com/TomasPalsson/worklog/commit/3fcdff545fc114a37cecc48466f3f5cda491c7d8))
* **web:** T012 settings routing controls, hard rules, source status ([dcca57f](https://github.com/TomasPalsson/worklog/commit/dcca57f3e904e9c73d81cf0c39cba7d8298e6129))
* **web:** the day strip shows work only — no personal time ([9077afd](https://github.com/TomasPalsson/worklog/commit/9077afd4e12e7da03868d1fc792b5c1ca4cc2809))
* **web:** zero-touch day page — quiet summary + review drawer, block clue line, attention line ([fd9e317](https://github.com/TomasPalsson/worklog/commit/fd9e317131e1e4533c09f60dfe943a1a016c3c4d))


### Bug Fixes

* **billing:** map a submodule's GitHub repo to the work folder that vendors it ([b93e080](https://github.com/TomasPalsson/worklog/commit/b93e0808eaaae11ede43a229c3cc856c7deda277))
* **cli:** worklog day collects slack, shell, and reflog again ([83e4f4f](https://github.com/TomasPalsson/worklog/commit/83e4f4f1d596632f4fb550f16642878568a18ecb))
* **collectors:** count only prompts the owner typed, never headless runs or system notifications ([d9a2071](https://github.com/TomasPalsson/worklog/commit/d9a20717c77d39dbb604c00e7b88b0053e6c9be6))
* **collectors:** keep shell command text out of project_path ([cee2052](https://github.com/TomasPalsson/worklog/commit/cee20522623b34bbdd42c2de0c68174bb0711452))
* **collectors:** read transcripts still being written after the day ends ([b52119e](https://github.com/TomasPalsson/worklog/commit/b52119ef4b7d8456c4ae4619616e04f47bda7d11))
* **collectors:** reflog reads worktree and submodule logs, not just the main repo ([45e541c](https://github.com/TomasPalsson/worklog/commit/45e541c5b6060c5dc74de759bf4d91d71dff0d5b))
* **collectors:** store only a sanitised program name as the shell event title ([7612edf](https://github.com/TomasPalsson/worklog/commit/7612edfb35a5d43d11465581a301923fe4e8bcd9))
* **daemon:** day summary reports the folded billing folder, not just the raw path ([3262147](https://github.com/TomasPalsson/worklog/commit/32621471c097e1801cb65cdd7239853ad2335df8))
* **daemon:** mask slack_user_token in GET /settings ([f867a21](https://github.com/TomasPalsson/worklog/commit/f867a2192f4ff575dfa23262d5af8f255e305fc4))
* **daemon:** read settings off the async workers; raise default abstain margin to 1.20 ([1f8013d](https://github.com/TomasPalsson/worklog/commit/1f8013d080d1dd6bc7e7c0b06de82333ce169515))
* **daemon:** stop holding the sqlite mutex across the claude shell-out ([#41](https://github.com/TomasPalsson/worklog/issues/41)) ([5d16542](https://github.com/TomasPalsson/worklog/commit/5d16542a16a119bad8467683b17fe928c05fc419))
* **infer:** a rebuild never fuses two ticketed blocks, so no ticket is lost ([f3eee4f](https://github.com/TomasPalsson/worklog/commit/f3eee4f6a67ac72968e3acbbeafce4a502556168))
* **infer:** a saved split divides the worked minutes, wherever the idle stretches fall ([1665307](https://github.com/TomasPalsson/worklog/commit/1665307953388e87c69175c2b647b78286dda67b))
* **infer:** a saved split only moves work minutes, never personal time ([631da71](https://github.com/TomasPalsson/worklog/commit/631da71957ee1e3baf5f2b712b06a8e0cb52491b))
* **infer:** a saved split re-cuts the work time the day already had, so it never adds time ([0e43946](https://github.com/TomasPalsson/worklog/commit/0e439461168a72a7604ac6ea86752ae3a48fc890))
* **infer:** blocks follow a saved split, spanning the minutes it hands each project ([990421e](https://github.com/TomasPalsson/worklog/commit/990421ed156ecac90de9cd55240ee0cabf89f302))
* **infer:** client work claims a minute before any personal project ([62105e6](https://github.com/TomasPalsson/worklog/commit/62105e6b41b66e9688d1802da49d6cd260765ebf))
* **infer:** each minute belongs to one project, so parallel sessions never double-bill ([bd00b97](https://github.com/TomasPalsson/worklog/commit/bd00b97519333a4b02dea04948aee508c2ed86d8))
* **infer:** each project's block spans the minutes it owns; quick hops join a neighbour instead of vanishing ([bde2a80](https://github.com/TomasPalsson/worklog/commit/bde2a80e6d643e09eafefbee59b16e04e35fc77e))
* **infer:** focus follows the owner's latest action; Claude working in the background only fills idle minutes ([b40dc92](https://github.com/TomasPalsson/worklog/commit/b40dc92bc240d4e701db9a9bf1fd6443c703deac))
* **infer:** split pieces link their own project's events, so the saved block keeps its project ([5c924e3](https://github.com/TomasPalsson/worklog/commit/5c924e3cfd95107c8dfeae2a818e930804cf4267))
* **infer:** worklog day and worklog infer keep saved splits (the 15-min schedule undid them) ([c91ebcf](https://github.com/TomasPalsson/worklog/commit/c91ebcf02fc3e41caff725afe16acf4b499f44d0))
* **laya:** run the real laya Router in the helper script ([718f692](https://github.com/TomasPalsson/worklog/commit/718f692d29aa72a0ac38113fd1ae0965b644f215))
* **repo:** preserve routing label through re-collect ([5be7681](https://github.com/TomasPalsson/worklog/commit/5be7681b401638e3197058df778d3bbd0af9c81e))
* **routing:** file exact repo/path mentions by rule before the classifier ([737d516](https://github.com/TomasPalsson/worklog/commit/737d516ed3c0e71e545a71336bee61a0a6636c99))
* **routing:** match named projects in the visited URL or own message, never the page title ([fb3a1a9](https://github.com/TomasPalsson/worklog/commit/fb3a1a9365273aff0af7df3852da567c26d2b951))
* **routing:** propagate edited always-rule folder to rule-labelled events ([c9bab1a](https://github.com/TomasPalsson/worklog/commit/c9bab1aa7323f7b953e8b21ecfd6fb010955d7bc))
* **routing:** reject always-rule when kind mismatches event source ([36a7d95](https://github.com/TomasPalsson/worklog/commit/36a7d95b38efc706959c9518f99e39ce3a0d8417))
* **schedule:** never repoint the real collect agent at a temp-dir binary (updater tests broke it) ([16b5daf](https://github.com/TomasPalsson/worklog/commit/16b5dafa4240b2dbc9cf994a586552d7fab2028a))
* **slack:** paginate search.messages and isolate per-message failures ([550b63b](https://github.com/TomasPalsson/worklog/commit/550b63b4694174538b3d26f331c2fb3a2803bafd))
* **slack:** skip empty profile names when titling DMs ([ff91dac](https://github.com/TomasPalsson/worklog/commit/ff91daca26c0f61ff4cee8d437d8fc498831a572))
* **verdict:** never guess a lone option and ask with the measured wording ([fbefc11](https://github.com/TomasPalsson/worklog/commit/fbefc1152dbfc483389018764200013de0229a46))
* **verdict:** T002 never send a Choice with fewer than 2 options ([22a3a36](https://github.com/TomasPalsson/worklog/commit/22a3a36fa0457c6bd906bffc1cd89b432a1c7734))
* **web:** colour day-strip segments per project and show a short Lunch/Away label on narrow gaps ([31b381d](https://github.com/TomasPalsson/worklog/commit/31b381de0c7320c2f98bca29baf0e4e364cd9b88))
* **web:** give labelled routed rows the same two-column layout as unsorted rows ([4709a48](https://github.com/TomasPalsson/worklog/commit/4709a4856159c4096ebc424be605debc30d378e7))
* **web:** lanes-view band misalignment and full per-project activity ([f990daf](https://github.com/TomasPalsson/worklog/commit/f990daf9423933ad50350105812dc355d6685c1c))
* **web:** lay out unsorted rows so the rule checkbox and picker sit under the event text ([bd6f96a](https://github.com/TomasPalsson/worklog/commit/bd6f96a7d4db7d025865a78b11dd65269e7563dc))
* **web:** plain names for Claude prompt and working clues ([4e92682](https://github.com/TomasPalsson/worklog/commit/4e92682a5ac9d2f5a87e10e75fee3b6c29866cd1))
* **web:** point the container healthcheck at IPv4 loopback ([0742476](https://github.com/TomasPalsson/worklog/commit/0742476c55eb811ce6a3d86bb2f770d8da35fc03))
* **web:** pressing a slider or button in the split box no longer starts a new drag ([1130bdb](https://github.com/TomasPalsson/worklog/commit/1130bdb257ef8ec0a67b78e49e62980cc0fe0024))
* **web:** rename settings ratio fields to abstain_margin/runner_up_ratio ([e40a8e7](https://github.com/TomasPalsson/worklog/commit/e40a8e746dede541aa69424d88463f172c2e8735))
* **web:** space the day's project colours by golden angle so busy projects never share a hue ([b170a92](https://github.com/TomasPalsson/worklog/commit/b170a9268f52c4d82e591d75ea1c310f4c2482dc))
* **web:** T011 remount UnsortedList on day change ([0b5f04f](https://github.com/TomasPalsson/worklog/commit/0b5f04f32f540de590ca240b3f2fe2b4c0a1c61b))
* **web:** T012 review round 1 — blank threshold no longer saves 0, tighten reachable assertion ([e17d9a6](https://github.com/TomasPalsson/worklog/commit/e17d9a685ec67759f6d8de3eafd54d165c68a686))
* **web:** use typographic quotes in the sort tray hint so the build lints ([616ed01](https://github.com/TomasPalsson/worklog/commit/616ed01361fd0d92a67de37d43f93b5644fb6853))


### Documentation

* **specs:** 003 amend — heartbeat CORS preflight, Slack DM names; CHK002 evidence ([be83095](https://github.com/TomasPalsson/worklog/commit/be83095ab6e7ce75d4141cdc5f6391b49186b363))
* **specs:** 003 approved by user ([1b7863b](https://github.com/TomasPalsson/worklog/commit/1b7863ba76dde1b07bee80e8137241f0d0456ee8))
* **specs:** 003 checkpoints wait for the tasks they check ([ccb6bb0](https://github.com/TomasPalsson/worklog/commit/ccb6bb0f564acd76f18e3eea020ef53d358101e4))
* **specs:** 003 CHK001 pre-check evidence ([246ac68](https://github.com/TomasPalsson/worklog/commit/246ac680d09787e3310cc8ceb70c6ef6ba969b44))
* **specs:** 003 CHK001 user-verified ([6d8b882](https://github.com/TomasPalsson/worklog/commit/6d8b882baf83df6f6784c25f1547f029e5add749))
* **specs:** 003 contract for the Firefox add-on ([3238b06](https://github.com/TomasPalsson/worklog/commit/3238b06f754692f3becd6fca3198efee7d868310))
* **specs:** 003 gates green, PASS-4709a48 ([21f964f](https://github.com/TomasPalsson/worklog/commit/21f964f2563223ec4500ecf58b2b7cbcd4691184))
* **specs:** 003 note phase 2 test command ([f365a22](https://github.com/TomasPalsson/worklog/commit/f365a22c6a633fd6fea0e0cfb47092ff1014235f))
* **specs:** 003 notes — phase 5 test filter, missing slop-check ([d968c96](https://github.com/TomasPalsson/worklog/commit/d968c96bf60824c2494f9666e7d1deb2ba8568b3))
* **specs:** 003 prep awaiting lint ruling ([c127063](https://github.com/TomasPalsson/worklog/commit/c127063a0b59b3083959eabd854b358509e5333f))
* **specs:** 003 prep ready for spec (user kept one file over 12-question lint) ([73e0a7f](https://github.com/TomasPalsson/worklog/commit/73e0a7f8a42f51e2fbcc228edee781205fe80975))
* **specs:** 003 re-tick CHK001 against manifest commit ([f60167b](https://github.com/TomasPalsson/worklog/commit/f60167b3be47da7d72b3749f83ef86d156c142e9))
* **specs:** 003 spec, design, tasks + routing contract module ([0fa7830](https://github.com/TomasPalsson/worklog/commit/0fa7830d1862e14b404853065ceac0c4f5d093ac))
* **specs:** 003 tick CHK001 ([b2b064b](https://github.com/TomasPalsson/worklog/commit/b2b064bb357d2f5497c2e85228a56de7f561058b))
* **specs:** 003 tick CHK003 (user verified day page) ([830c4a7](https://github.com/TomasPalsson/worklog/commit/830c4a71674cc59deb48a1189be2130338cc0e6a))
* **specs:** 003 tick T003 (daemon routes) ([b6d3771](https://github.com/TomasPalsson/worklog/commit/b6d377174311ea5f1e0868466ac111ff1a1b9bda))
* **specs:** 003 tick T006 (Laya client + helper) ([4645778](https://github.com/TomasPalsson/worklog/commit/4645778cfc5075709aebb5a327afaf7a9389c0b3))
* **specs:** 003 tick T007 (route before inference) ([d4cdc75](https://github.com/TomasPalsson/worklog/commit/d4cdc757b990b2dd9fb72cd66b50a8016bd1c0c2))
* **specs:** 003 tick T010 (web routing types + actions) ([0433e11](https://github.com/TomasPalsson/worklog/commit/0433e11e8de19f82aed87e880ef282a230ec4995))
* **specs:** 003 tick T011, T012 (unsorted list, settings) ([12c6f73](https://github.com/TomasPalsson/worklog/commit/12c6f73fad866f19dbaf39807600567fbf8c335f))
* **specs:** 003 tick T013 (heartbeat CORS preflight) ([51d853f](https://github.com/TomasPalsson/worklog/commit/51d853f5c4f258bac32c1d543e4a948f7fadf2cb))
* **specs:** 003 tick T014 (Slack DM names) ([f12df60](https://github.com/TomasPalsson/worklog/commit/f12df601b63a239481db5a0e9d555779a0ce5abb))
* **specs:** 003 wave 1 ticked (schema v11, Firefox add-on) ([893323a](https://github.com/TomasPalsson/worklog/commit/893323ae9cdbcc312f21c702e55fdff835fa673e))
* **specs:** 003 wave 2 ticked (browser ingest, Slack collector, router core) ([5cb926d](https://github.com/TomasPalsson/worklog/commit/5cb926d884013aa606cdb7b339b34ed626d3427d))
* **specs:** 004 approved and verified by user, gates green ([7dcef4e](https://github.com/TomasPalsson/worklog/commit/7dcef4ee922ad24b06b892b231e7d73665f805bd))
* **specs:** 004 real-day verification, ticks and notes ([e913371](https://github.com/TomasPalsson/worklog/commit/e9133714d370e1a31fdc067744379dab466a2532))
* **specs:** 004 tick T001, progress and notes ([4bb8246](https://github.com/TomasPalsson/worklog/commit/4bb82469ad0412fff733eee268d27e3db5fb1ef9))
* **specs:** 004 Verdict routing spec, design and tasks ([eb435ef](https://github.com/TomasPalsson/worklog/commit/eb435ef1d3b7579acf99bf3e3a0b29cf87cfe876))
* **specs:** day page screenshot with confidence badges and gap rows ([43e4c4d](https://github.com/TomasPalsson/worklog/commit/43e4c4d50a261d738a6be756ae5d3914258127a3))
* **specs:** gap audit across every readable local source ([7d71808](https://github.com/TomasPalsson/worklog/commit/7d71808ae676f3604919041ab5a9f7ac8b4c95b5))
* **specs:** gap audit across every readable local source ([86bee78](https://github.com/TomasPalsson/worklog/commit/86bee786ce55fed0692bc14a28f40c4ba4364d65))
* **specs:** inventory of local timeline data sources ([f3bed27](https://github.com/TomasPalsson/worklog/commit/f3bed2789ac4fbc030f1315057d1eb5e58056ff1))
* **specs:** prep for browser + Slack event routing (003) ([1dbb2f8](https://github.com/TomasPalsson/worklog/commit/1dbb2f8d8544639895112659ea1ed7b5343ac1e6))
* **specs:** timeline report — day page, review fix, gates ([45af703](https://github.com/TomasPalsson/worklog/commit/45af703515a0090b1dfcd0d0acd74e33fb987423))
* **specs:** timeline report for the first day with shell and reflog sources ([08052fe](https://github.com/TomasPalsson/worklog/commit/08052fed1209fb65ad0ed7bc1d7de198f9fa0639))
* **specs:** user stories for the zero-effort day view ([ca18a09](https://github.com/TomasPalsson/worklog/commit/ca18a097bf0a30c04fcd5e5671d8a43f86033037))
* **specs:** zero-touch day design and status ([9f52582](https://github.com/TomasPalsson/worklog/commit/9f5258263083d41d840ca5104139ee3b06f916c4))
* **specs:** zero-touch status — attention-first split, adjustable overlaps ([be2b86b](https://github.com/TomasPalsson/worklog/commit/be2b86bdf6658f90ca5aabee846a07f2f03713c5))

## [0.11.2](https://github.com/TomasPalsson/worklog/compare/v0.11.1...v0.11.2) (2026-07-31)


### Bug Fixes

* **daemon:** vote the block label on project root so it matches billing ([#39](https://github.com/TomasPalsson/worklog/issues/39)) ([d863e89](https://github.com/TomasPalsson/worklog/commit/d863e899f1481fd95f45818cf93a1cd67a41a176))
* **web:** report the bun UI in `worklog status`, clear legacy container on down ([#38](https://github.com/TomasPalsson/worklog/issues/38)) ([56bc456](https://github.com/TomasPalsson/worklog/commit/56bc456640e18eb289de554e6ef6e7f3dd7393f6))

## [0.11.1](https://github.com/TomasPalsson/worklog/compare/v0.11.0...v0.11.1) (2026-07-31)


### Bug Fixes

* **ci:** chain release build off release-please via workflow_call ([#35](https://github.com/TomasPalsson/worklog/issues/35)) ([a968f46](https://github.com/TomasPalsson/worklog/commit/a968f46517c9882b626b906ff88a4b96c52edc7c))
* **web:** stop serving a frozen web/ cache after upgrades ([#37](https://github.com/TomasPalsson/worklog/issues/37)) ([6c546f1](https://github.com/TomasPalsson/worklog/commit/6c546f16794f20dffc93392463a64cadda2a8cbf))

## [0.11.0](https://github.com/TomasPalsson/worklog/compare/v0.10.0...v0.11.0) (2026-07-28)


### Features

* **billing:** invoicing-form export + UI-editable customer/folder registry ([#33](https://github.com/TomasPalsson/worklog/issues/33)) ([143f583](https://github.com/TomasPalsson/worklog/commit/143f58397dcf228da6b2fe954397edc3afa42728))
* **cli:** UX overhaul phase 3 — auto-merge, status dashboard, Docker-free day ([1c45f27](https://github.com/TomasPalsson/worklog/commit/1c45f27a11a8d0e785bfdd82b7eabdb301fa9f02))
* **cli:** UX overhaul phases 1-2 — block editing + day/week queries ([39a23e7](https://github.com/TomasPalsson/worklog/commit/39a23e78218f0e63b4ed014ffccce00a79f74cf2))
* **cli:** worklog completions &lt;shell&gt; — shell completion scripts ([9f9ea23](https://github.com/TomasPalsson/worklog/commit/9f9ea2390c9cd8f287cbb9c7fd86c90a431d1f1f))
* **daemon:** add /tickets/search + /tickets/external ([f55e04f](https://github.com/TomasPalsson/worklog/commit/f55e04f78f4c47acee4dfb90f27ee76e84470150))
* merge same-ticket blocks and describe with Claude (events + commits) ([#23](https://github.com/TomasPalsson/worklog/issues/23)) ([0be24a5](https://github.com/TomasPalsson/worklog/commit/0be24a5ca5eb69f864f8e57f13d81bb8c6682836))
* **purge:** automatic billing-cycle pruner ([#34](https://github.com/TomasPalsson/worklog/issues/34)) ([34cb176](https://github.com/TomasPalsson/worklog/commit/34cb176ee1cbfbdae7ff37015ccbaec04ae0b33e))
* set a block personal/work from the review UI ([0503ea2](https://github.com/TomasPalsson/worklog/commit/0503ea2e885108fbc7f7b3a59540877656067cb2))
* **skill:** bundled Claude Code skill for operating worklog ([641bd60](https://github.com/TomasPalsson/worklog/commit/641bd607596a41e29d54d94a1ef0a31c8058e891))
* **sync:** aggregate same-ticket blocks into one Tempo worklog ([a0afbdf](https://github.com/TomasPalsson/worklog/commit/a0afbdf5224850549d6d9ddb2c7a310bb9e032da))
* **web:** collapse day view into per-ticket groups ([80b8acb](https://github.com/TomasPalsson/worklog/commit/80b8acbf55a2232c4f7971135197da17bddccbcb))
* **web:** live Jira ticket search in the picker ([07741b1](https://github.com/TomasPalsson/worklog/commit/07741b1aa6dcb65708d1e38f2f81c018ef9501c5))
* **worklog-core:** external column + jira search backend ([4c80693](https://github.com/TomasPalsson/worklog/commit/4c806933ef7d5cd8c76aecb6cd377b6b867fe719))


### Bug Fixes

* **estimate:** redact code from event content before it reaches claude -p ([5a155f6](https://github.com/TomasPalsson/worklog/commit/5a155f6b4b118cb9b5a2e7b177befac4b695b08a))
* **hook:** add hook_run::SUPPRESS_ENV the estimator references ([9bd4a5e](https://github.com/TomasPalsson/worklog/commit/9bd4a5ec9ac05bafacaa0fe93eeb57d4101ba14e))
* **hook:** stop storing code in the events database ([08c63e6](https://github.com/TomasPalsson/worklog/commit/08c63e6e4795f9c9799f7ed205da52d63ba2f2d8))
* **hook:** sweep worklog handlers off legacy event keys on install ([5166045](https://github.com/TomasPalsson/worklog/commit/5166045d58b67311a3adaa54a0ba2753ce23fd36))
* **infer:** stop fragmenting long Claude turns into dropped 2-min slivers ([#21](https://github.com/TomasPalsson/worklog/issues/21)) ([7ca11cc](https://github.com/TomasPalsson/worklog/commit/7ca11cc1094bf5c9453e5e01f6947115ebbd4178))
* reject phantom tickets (CRIT-1) and stop double-billing overlaps ([20fdb68](https://github.com/TomasPalsson/worklog/commit/20fdb68849b6715453da152cc0c262e4f5db55a0))
* **tests:** serialise WORKLOG_HOME and unlock flock explicitly ([60b9897](https://github.com/TomasPalsson/worklog/commit/60b9897fd3958a3815c18288d179b1dd017dfd99))

## [0.10.0](https://github.com/TomasPalsson/worklog/compare/v0.9.0...v0.10.0) (2026-05-12)


### Features

* **estimate:** add LiteLLM provider alongside claude -p subprocess ([#7](https://github.com/TomasPalsson/worklog/issues/7)) ([94c064e](https://github.com/TomasPalsson/worklog/commit/94c064eab7744195869bae177b645b9deed1088d))

## [0.9.0](https://github.com/TomasPalsson/worklog/compare/v0.8.0...v0.9.0) (2026-05-12)


### Features

* **web:** /week — 7-column week view with calendar jumper ([#14](https://github.com/TomasPalsson/worklog/issues/14)) ([8764029](https://github.com/TomasPalsson/worklog/commit/8764029bccf283fbecb960f0e96fd4816a6fdd54))

## [0.8.0](https://github.com/TomasPalsson/worklog/compare/v0.7.0...v0.8.0) (2026-05-12)


### Features

* **cli:** interactive console week view ([#10](https://github.com/TomasPalsson/worklog/issues/10)) ([c694d95](https://github.com/TomasPalsson/worklog/commit/c694d954a7298c917d159d6cd7e20aac298ad2e0))
* per-block git-commit sidecar ([#11](https://github.com/TomasPalsson/worklog/issues/11)) ([dcf0f1d](https://github.com/TomasPalsson/worklog/commit/dcf0f1d6350df3defabe5552954bfcdf0bf2fa54))

## [0.7.0](https://github.com/TomasPalsson/worklog/compare/v0.6.0...v0.7.0) (2026-05-12)


### Features

* auto-launch web UI after setup, open browser by default ([b91179d](https://github.com/TomasPalsson/worklog/commit/b91179d9e2818f772349c516155f501e7bf4985e))
* **cli:** auto-launch web UI + browser at end of setup ([8ea9a15](https://github.com/TomasPalsson/worklog/commit/8ea9a1563cd1c54ea45929421232dc54e36cecf5))
* **core:** add browser::open_url cross-platform opener ([fe1260e](https://github.com/TomasPalsson/worklog/commit/fe1260e9ead4cf62a1a1bb924421989739efe23e))
* **estimate:** conservative tickets + merge adjacent same-ticket blocks ([496ea7d](https://github.com/TomasPalsson/worklog/commit/496ea7d367ced95ec2af12fec795427fc438a20f))
* **infer:** split blocks at sustained project_path transitions ([d6beef5](https://github.com/TomasPalsson/worklog/commit/d6beef59db21b171753203c9f097799d527d5dec))
* **personal:** auto-classify and dim personal-project blocks ([022a66a](https://github.com/TomasPalsson/worklog/commit/022a66a2349aa77218e7ab901882fff1aa762280))
* **sync:** dirty-flag local edits + PUT existing Tempo entries ([b1ea7be](https://github.com/TomasPalsson/worklog/commit/b1ea7be6abe1dc8392dae931dae1f08141bc82a4))
* **web:** host-bun runner replaces Docker + collapse personal blocks ([a6ab48d](https://github.com/TomasPalsson/worklog/commit/a6ab48d45562b0705f4c446454fe4b2a4f877238))


### Bug Fixes

* **estimate:** allow cross-language project↔ticket matching ([46c7a1a](https://github.com/TomasPalsson/worklog/commit/46c7a1aa2d0343c0efe9040d2a507f7785fbd5cc))
* **estimate:** read JSON from envelope.structured_output ([c555e92](https://github.com/TomasPalsson/worklog/commit/c555e92869a8185167993b89b68c46535917e118))
* **secrets:** skip env-first lookup when file-backed shim is active ([29aca9a](https://github.com/TomasPalsson/worklog/commit/29aca9a4fc3d623ed2391f89b447f57c1084efff))
* **sync:** DELETE Tempo worklog when block is deleted locally ([b89d22a](https://github.com/TomasPalsson/worklog/commit/b89d22a0b439791460c6fc26bf2f8740ac67489e))
* **sync:** tempo v4 wants issueId + accountId, ticket overrides personal ([1c74552](https://github.com/TomasPalsson/worklog/commit/1c74552233f6e4a574e58f9c0c8d0582edd0c2d7))
* **web:** clearer sync toast + always rebuild on web up ([a1adfec](https://github.com/TomasPalsson/worklog/commit/a1adfec44d824ebabe50feb1b347ff336f4c7af2))

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
