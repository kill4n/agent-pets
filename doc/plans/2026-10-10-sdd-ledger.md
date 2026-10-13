# SDD ledger — plan: doc/plans/2026-10-09-buddy-v1.md
Branch: buddy-v1 (from 1839041). Spec: doc/specs/2026-10-07-buddy-design.md (readable).

## Preflight scan
| Pair / Task | Produces vs consumes | Found |
|---|---|---|
| T2→T3,T8,T9 | HookEvent/PolledAgent/AgentEvent fields | names match in all tests |
| T3→T9,T12 | Visible snake_case serde ↔ ui/shared/types.ts | match |
| T4→T11,T12 | Manifest camelCase (frameSize) ↔ pack.json ↔ TS type | match |
| T5,T6→T11 | install(path,url,token), Config.hook_url/bind_addr | match |
| T7,T8→T11 | spawn_poller(host,tx,errors), serve(addr,host,token,tx) | match |
| T9→T11 | Store.apply/sweep/views, Change, AgentView.key | match |
| T10→T11 | Positions.get/set/cascade, Pos | match |
| T11→T12,T13 | command names in generate_handler! ↔ invoke strings | match (get_status used by UI; plan interface list in T11 says hooks_installed but code defines get_status — code wins) |
| T1↔T13 | capabilities list autostart perms, window labels sprite-*, settings | match |
| T3 self | tests vs code: 16 cases traced by hand | consistent |
| T9 self | tests vs code traced | consistent |
| T10 self | cascade test expects p1 = p0 - (40,30); code: col=1 → x-40, y-30 | consistent |
Ruling: T11 interface comment lists `hooks_installed` command but code/UI use `get_status` — use `get_status` — cost if wrong: none (comment only).

Task 1: fix round 1/5 (1 addressed, 0 open — wrong commit trailer; amended 638d30a→0bcf1b5, src-tauri/gen untracked)
Ruling: skipped dispatching a scoped re-review for round 1 — the fix is commit metadata + .gitignore, verified directly with git log/ls-files — cost if wrong: none beyond noise.
Task 1: minor (deferred): mobile icons (android/ios) committed; esbuild postinstall warning on npm install.
Task 1: complete (commits 1839041..0bcf1b5, review clean)

Task 2: minor (deferred): parse_snapshot drops undeserializable entries silently (add log::warn); wrong-typed fields drop the entry instead of Unknown; unused Serialize derives on poll enums.
Task 2: complete (commits 0bcf1b5..a7586bb, review clean)

Task 3 review: 2 Important. Ruling: (1) remove the `s.visible != Gone` exception in apply_poll so a recent SessionEnd keeps Gone for the 5 s hook window; reappearance after missed-poll Gone still works because no recent hook exists then — cost if wrong: an agent that dies and is relisted within 5 s shows gone briefly. Ruling: (2) spec's "waiting_peer exits on poll busy" refers to leaving the state, not to clearing the pending flag while still working; poll Busy clears sent_message/peer only when visible == WaitingPeer — cost if wrong: a stale waiting_peer if the agent kept working after SendMessage and never Stop'ed (self-heals on next UserPromptSubmit).
Task 3: minor (deferred): SubagentStop→0 forces Working instead of Idle; lost SubagentStop leaves Delegating stuck; waiting_for not cleared on Stop/poll Idle; SendMessage without recipient overwrites peer; missing tests for Notification-while-Delegating, PermissionRequest with subagents, status None/Unknown, Background Unknown.
Task 3: fix round 1/5 (3 addressed, 0 open — SessionEnd vs poll; sent_message clearing; saturating_add; commits 8bf1d5d..6be601b)
Task 3: minor (deferred): poll Waiting can resurrect a just-ended (Gone) session as NeedsInput because the Waiting branch precedes the recent_hook check.
Task 3: complete (commits a7586bb..6be601b, review clean)

Task 4: minor (deferred): PackError Display is Debug-based; sheet reopened per animation; no tests for FramesOutOfRange/BadPng; sheet path not guarded against `..`; pack.json lacks trailing newline.
Task 4: complete (commits 6be601b..9ff9b78, review clean)

Task 5: minor (deferred, from automated security review): settings.json/backup written with default perms although they carry the bearer token. Ruling: token only guards a localhost status endpoint and ~/.claude is user-owned; keep for final review triage (write 0600 + atomic rename is cheap) — cost if wrong: token readable by other local users on shared machines.
Task 5 review: 3 Important (all plan-mandated). Ruling: fix all three against the plan text — atomic temp+rename write (0600 on unix, also covers the security finding), backup only when none exists, non-object/array shapes return InstallError::Shape instead of overwriting — the spec's "never corrupt the user's file" intent outranks the plan's simpler code — cost if wrong: slightly more code in installer.
Task 5: minor (deferred): whole-group removal when a foreign hook shares our group; Display is Debug-based; untested CLAUDE_CONFIG_DIR.
Task 5: fix round 1/5 (6 addressed, 0 open — atomic write, one-time backup, Shape errors, preserve_order, empty-array pruning, uninstall no-op; commits ceec482..ae00010)
Task 5: minor (deferred): tmp file briefly at umask perms before chmod; uninstall rewrites file even when nothing removed.
Task 5: complete (commits 9ff9b78..ae00010, review clean)

Task 6: minor (deferred): save creates file before chmod (brief umask window); token validity is length-only; load does not persist a new token (Task 11 wiring must save after load — it does); relative .buddy fallback.
Task 6: complete (commits ae00010..6d358f1, review clean)

Task 7: minor (deferred): wait error labeled Spawn; receiver-drop shutdown detected only after a success (check tx.is_closed()); dedup only vs last error; stdin not nulled; spawn_poller untested.
Task 7: complete (commits 6d358f1..aeccd40, review clean)

Task 8: minor (deferred): non-constant-time token compare; full-channel drops unlogged; no tests for bind failure/full channel/oversize body; 401 tests don't assert non-delivery.
Task 8: complete (commits aeccd40..fa72536, review clean)

Task 9: minor (deferred): mark_missed loop hand-builds AgentView with since_secs 0 instead of make_view(s, now); no tests for multi-host isolation, Gone not re-flipped, empty-sessionId snapshot entries.
Task 9: complete (commits fa72536..46568cc, review clean)

Task 10: Ruling: plan's cascade used `rem_euclid(start_y)` which yields y=0 at index 0 (plan bug); implementer's `.max(0)` clamp accepted — cost if wrong: many sprites stack at the top edge instead of wrapping.
Task 10: minor (deferred): host:name key can collide if host contains ':'; positions write not atomic; no exact wrap test.
Task 10: complete (commits 46568cc..428c5df, review clean)

Task 11 (in progress): minor (deferred, automated security review): get_pack_url joins caller-supplied `sheet` under packs_root without validation. Ruling: callers are the app's own webviews, but restricting `sheet` to the loaded manifest's sheet names and `name` to list_packs() is cheap — queue for final fix wave — cost if wrong: a compromised webview could read arbitrary files as base64.

Task 11 review: 2 Important (plan-mandated). Ruling: fix both — release the positions lock before Tauri main-thread round-trips and make report_position async; after pack fallback set cfg.pack = "default" in memory — cost if wrong: none material. Ruling: also fold in minors 3 (validate `sheet`/`name` path components; covers the security finding), 4 (log failed config save), 5 (guard prevent_exit with code.is_none(), load-bearing for Task 13 Quit), 7 (dedupe select! arms), 8 (drop store lock before emit). Ruling: the brief's "status" event to the settings window is dropped — Task 13's UI polls get_status instead; "manifest" event payload stays bare Manifest — cost if wrong: none.
Task 11: minor (deferred): two tokio runtimes (could tauri::async_runtime::set); lock().unwrap() poisoning cascade; bind failure not surfaced in status; image/png mime hardcoded.
Task 11: fix round 1/5 (7 addressed, 0 open — deadlock, pack fallback, input validation, save log, prevent_exit guard, mirror helper, lock before emit; commits bcd7654..c2d4b13)
Task 11: complete (commits 428c5df..c2d4b13, review clean)

Task 12 (in progress): automated security review flagged innerHTML interpolation in ui/sprite/tooltip.ts. Ruling: real — name/cwd/peer come from poll output and hook payloads, and the webview can invoke Tauri commands; must be fixed in Task 12's fix round (DOM/textContent) and Task 13's settings UI must not use innerHTML with agent data — cost if wrong: a crafted session name could run JS inside the sprite webview.

Task 12 review: 3 Important (plan-mandated). Ruling: fix all — tooltip built with DOM/textContent; register listeners before loadManifest; report position from a debounced window onMoved instead of after startDragging — cost if wrong: none material. Visual check of ?mock pages not possible from this session (no browser tools); deferred to the user in Task 14 / final report.
Task 12: minor (deferred): getContext('2d')! assertion; decode errors swallowed; large dt spin in Animator; tooltip clipped by window width.
Task 12: fix round 1/5 (4 addressed, 0 open — DOM tooltip, listener order, onMoved position, small items; commits 6df0018..1cfb3f7)
Task 12: complete (commits c2d4b13..1cfb3f7, review clean; runtime webview behaviour pending user visual check)

Task 13: ⚠️ resolved: capabilities/default.json windows list includes "settings" (verified by controller).
Task 13: minor (deferred): full re-render on `agents` event drops focus/open dropdown; refresh/agents race; no .catch on status poll and initial refresh; open_settings swallows build error; tray icon as template renders colored icon as silhouette; alert() in webview.
Task 13: complete (commits 1cfb3f7..5db2a90, review clean)

Task 14 review: Ruling: the "wrong trailer" finding is incorrect — this session's attribution rule is `Claude Fable 5.1`; the reviewer saw a different reminder in its own context; commits stand. Ruling: the "per-state results overstated" finding is accepted as a known evidence gap — headless curl only proves hook acceptance; state transitions are covered by the state-machine/store unit tests and the six-state visual walkthrough is handed to the user — cost if wrong: a wiring bug between hook ingestion and sprite animation would surface only in the user's visual check. The ~13 s close (10 s grace + 1 s sweep tick + window close) is within expectation.
Task 14: complete (commits 5db2a90..0e1b51e, 2 findings ruled)
All 14 tasks complete. Proceeding to final whole-branch review.

Final review (opus): 3 Critical (C1 emit broadcasts to all sprites; C2 bundled resources path `_up_` + expect panic; C3 finished background agents re-created as phantom gone windows), 6 Important (I1 saved positions not applied after rename; I2 lost SubagentStop sticks Delegating; I3 claude not on PATH for GUI launch; I4 macOS Dock/Cmd-Tab activation policy; I5 server bind error/pack fallback not surfaced + no single-instance; I6 tooltip clipped), 14 Minor. Deferred-minor triage promoted T3/T11/T12 items into I2/I5/I6.
Ruling: ONE fix wave covering C1–C3 and I1–I6; minors stay deferred — cost if wrong: one more review round.
Final fix wave: 9/9 addressed (commit cc87b46), re-review clean. Controller re-ran cargo test (88+1 pass, no warnings) and npm run build (clean).
Deferred minors remaining (CAN WAIT per final triage): see Task 1–13 minor lines above plus final-review M-items.
