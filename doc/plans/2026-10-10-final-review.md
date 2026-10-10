# Final whole-branch review: buddy-v1 (1839041..0e1b51e)

Reviewer passes: (1) spec, plan Global Constraints and Review Focus, ledger; (2) Rust core module by module (collector, state, windows, installer, packs, config, commands, tray, lib); (3) UI (sprite, settings, shared types) plus config (tauri.conf.json, capabilities, vite, package.json, .gitignore, README); (4) checks against the Tauri 2.12.2 / tauri-utils 2.9.3 sources in `~/.cargo/registry` and `node_modules/@tauri-apps/api` for event targeting and resource-path semantics; (5) suite run: `cargo test` gave 72 unit + 1 integration passing, and `npm run build` (tsc + vite) was clean. Read-only apart from build outputs (`target/`, `ui/dist/`, both gitignored).

## Strengths

- The state machine is pure and well tested (`state/machine.rs`, 19 table-style tests). It covers hook priority within 5 s, 2-miss gone, SessionEnd within the hook window, SubagentStop saturation (Review Focus 5), and poll `waiting` beating inferred `waiting_peer`.
- `Store` diffing by fingerprint avoids redundant window traffic. The provisional-name test for Review Focus 1 is present.
- Installer safety is solid. It writes atomically (temp file then rename), writes 0600, takes a one-time backup, preserves symlinks, rejects unexpected shapes without writing, keeps order (`preserve_order`), removes only its own entries by URL, and prunes only arrays it emptied. Tests cover every rule in spec section 7.2 and Review Focus 3.
- The fixes in the security rounds are real. `get_pack_url` accepts a single path component that must be a sheet of the loaded manifest. `set_pack` checks against `list_packs`. The UI has no `innerHTML` anywhere: the tooltip and settings are built with `textContent`/DOM. config.json is 0600.
- Concurrency hygiene is good. No std `Mutex` is held across an `.await` or across a main-thread round trip. The comment in `ensure_window` documents the rule. `report_position` is async, and lock order is store before positions, consistently.
- Cross-module names agree: `Visible` snake_case matches `types.ts`; `frameSize` camelCase matches pack.json and TS; the 10 commands in `generate_handler!` match the `invoke` strings; window labels `sprite-*`/`settings` match the capability.
- The tray app keeps running with no windows (`prevent_exit` only when `code.is_none()`), and Quit still exits.
- The poller degrades cleanly when `claude` is missing, times out or prints bad JSON. It dedups the error and the settings window shows it.

## Issues

### Critical (Must Fix)

**C1. Every sprite window receives every agent's state, so with two or more sessions all sprites show the same agent.**
- Where: `src-tauri/src/windows/manager.rs:27` and `ui/sprite/main.ts:96`.
- What's wrong: `push_state` calls `w.emit("agent-state", v)` on a `WebviewWindow`. In Tauri 2, `Emitter::emit` "Emits an event to all targets" whatever object it is called on (verified at tauri-2.12.2 `src/lib.rs:962-978`; `impl Emitter for WebviewWindow {}` uses that default). On the JS side, `listen()` defaults to `{ kind: 'Any' }` (`@tauri-apps/api/event.js:138`). `setView(e.payload)` never checks `payload.key === key`.
- Why it matters: every sprite flips to the name, state and tooltip of whichever agent changed last. This breaks the core promise (spec section 1, "saber qué agente necesita atención"). Task 14's headless curl check could not catch it, and no test covers the emission target.
- Fix: in the manager use `app.emit_to(EventTarget::webview_window(label), "agent-state", v)` (or `w.emit_to(&label, ...)`). In `main.ts`, also guard with `if (e.payload.key !== key) return;` as defence in depth.

**C2. A bundled build cannot find its packs, and the default-pack `expect` then panics at startup.**
- Where: `src-tauri/tauri.conf.json` (`"resources": ["../assets/packs/**/*"]`), `src-tauri/src/lib.rs:31-36` and `lib.rs:55`.
- What's wrong: Tauri 2 rewrites every `..` component of a resource path to `_up_` (tauri-utils-2.9.3 `src/resources.rs:24`). The packs therefore land in `<resource_dir>/_up_/assets/packs/...`, but `packs_root` looks in `<resource_dir>/assets/packs`. When that path is missing, `load_pack(default).expect("default pack must be valid")` panics inside `setup`.
- Why it matters: this is hidden on the developer's machine because `packs_root` first tries the compile-time `CARGO_MANIFEST_DIR/../assets/packs`, which exists there, even in release builds. On any other machine, or after the repo moves, the installed app crashes on launch.
- Fix:
  - Use the map form `"resources": { "../assets/packs/": "assets/packs/" }`.
  - Gate the dev path behind `cfg!(debug_assertions)`.
  - Replace the `expect` with a logged error plus a tray warning. If no pack at all is available, keep the app running with no sprite rendering.
  - Add a test that the resource mapping and `packs_root` agree.

**C3. Finished background agents come back as phantom "gone" windows about every 12.5 s, forever.**
- Where: `src-tauri/src/state/store.rs:55-65,78-83`, `machine.rs:69-74` and `manager.rs:17`.
- What's wrong: the poller runs `claude agents --json --all`. The spec models background `state: done|failed|stopped` in `PolledAgent`, which implies finished background agents stay listed. Such an agent goes Gone, and `sweep` removes it after 10 s. The next poll still lists it, so `upsert` re-creates it, `apply_poll` sets it Gone, and the store emits `Change::Added(view{visible: gone})`. `apply_changes` then builds a new window for an Added change even when it is Gone. That window plays the fade, sits invisibly at a cascade slot catching clicks, is swept 10 s later, and the loop repeats. At startup, every historical finished background agent also gets a window.
- Why it matters: windows flicker and block clicks indefinitely for each finished job, and cascade indices drift.
- Fix:
  - In `Store::apply` (Snapshot), do not create an unknown agent whose first state is terminal: skip `PollState::Done|Failed|Stopped` when the key is absent.
  - Keep a small tombstone set of removed keys, cleared when the session reappears non-terminal or after a TTL.
  - Make `ensure_window` skip creation when `v.visible == Gone`.
  - Add a store test: background `done` across sweep followed by another snapshot yields no Added.
- Caveat: if `--all` never lists terminal agents this drops to Minor. The spec's own `PolledAgent` type says it does.

### Important (Should Fix)

**I1. Saved positions are never applied to new sessions when hooks are installed.**
- Where: `src-tauri/src/windows/manager.rs:34-36` and `commands.rs:38-42`.
- What's wrong: with hooks installed, `SessionStart` reaches the app before the next poll (up to 2.5 s later). The agent is created with the provisional name `session_id[:8]`, and `ensure_window` looks up `positions[host:abcdef12]`, finds nothing, and cascades. When the poll renames the agent to its real name, `ensure_window` returns early because the window exists, so the saved `host+name` position is never used. A drag made before the rename is also saved under the throwaway provisional key, which then accumulates in positions.json forever.
- Why it matters: spec section 6.1 says the initial position is the last known one for that `host + name`, and the README says "la posición se recuerda". Restarting Buddy with live sessions masks the bug because the first poll fires before any hook.
- Fix: on `Change::Updated` where the name changed, if `positions.get(host, new_name)` exists and the user has not dragged the window yet, `set_position` to it. In `report_position`, ignore drags while the name is still provisional, or re-key on rename. Add a test.

**I2. A lost SubagentStop leaves the sprite in Delegating permanently.**
- Where: `src-tauri/src/state/machine.rs:12-16,59-61`.
- What's wrong: `effective()` maps Working/Idle to Delegating whenever `subagents > 0`, and nothing ever resets the counter. `UserPromptSubmit`, `Stop` and poll `idle` all get rewritten to Delegating. A SubagentStop can go missing in several ordinary ways:
  - The user interrupts with Esc while a Task subagent runs; the Stop and SubagentStop hooks do not fire on user interrupts.
  - A hook is dropped by `try_send` on a full channel.
  - The hook is rejected with a 413.
- Why it matters: a stuck "delegating" sprite is exactly the false signal the app exists to avoid. This is the deferred Task 3 minor, promoted.
- Fix: reset `subagents = 0` on `UserPromptSubmit`, and on poll `idle` once outside the hook window, accepting that a background subagent may briefly show as not delegating. Add tests for both.

**I3. When launched as a GUI app (Finder, Dock or autostart), `claude` is usually not on PATH.**
- Where: `src-tauri/src/collector/poller.rs:21-23`.
- What's wrong: macOS GUI and LaunchAgent processes get `PATH=/usr/bin:/bin:/usr/sbin:/sbin`. `claude` normally lives in `~/.local/bin`, `/opt/homebrew/bin`, `~/.claude/local` or an npm global prefix. The poller then always fails, and the app runs hooks-only.
- Why it matters: in hooks-only mode, names stay as provisional `session_id[:8]`, so positions (keyed by name) never match. A session killed without SessionEnd never goes gone. The tray shows no warning (see I5). `BUDDY_CLAUDE_BIN` is undocumented and impractical to set for a GUI app. A real user hits all of this on first run of the installed app.
- Fix:
  - Resolve the binary once at startup: try `$SHELL -lc 'command -v claude'`, then the known install locations.
  - Allow a `claude_path` override in config.json.
  - Document the override in the README.

**I4. On macOS the app appears in the Dock and in Cmd-Tab.**
- Where: `src-tauri/src/lib.rs:45` (setup).
- What's wrong: `skip_taskbar(true)` has no effect on macOS, and the activation policy is never set to `Accessory`.
- Why it matters: spec section 6.1 says the windows are hidden from the taskbar and alt-tab. A tray pet that adds a Dock icon and a Cmd-Tab entry is not what a user expects.
- Fix: `#[cfg(target_os = "macos")] app.set_activation_policy(tauri::ActivationPolicy::Accessory);` in `setup`.

**I5. Collector failures are invisible, and a second instance duplicates every sprite.**
- Where: `src-tauri/src/lib.rs:72-79,53`, `tray.rs:7-24` and `commands.rs:48-54`.
- What's wrong:
  - If port 7433 is busy, `serve` only logs. `get_status` has no field for it, so the settings window reports "Hooks: instalados, Poll: ok" while every hook is refused.
  - There is no single-instance guard. Autostart plus a manual launch runs two Buddies: the second cannot bind, but it still polls and opens a second sprite for every session.
  - Spec sections 3.1, 5 and 8 require a tray warning for collector/poller errors and for pack fallback. The tray never changes, and pack fallback is only `log::warn`.
- Fix:
  - Add `tauri-plugin-single-instance`; on a second launch, focus settings.
  - Keep a `server_error: watch<Option<String>>` alongside `poll_error`, plus a `pack_warning`.
  - Expose them in `Status`.
  - Swap the tray icon or tooltip (`TrayIcon::set_tooltip`/`set_icon`) when any of them is set.

**I6. The tooltip is clipped to roughly 19 characters, so its key content never shows.**
- Where: `ui/sprite/index.html:6-8` and `src-tauri/src/windows/manager.rs:9-12`.
- What's wrong: the window is `frameSize*scale` wide (128 px for the default pack). The tooltip is `max-width:95%; white-space:nowrap; text-overflow:ellipsis` at 11 px, so each line fits about 19 characters. "necesita tu respuesta (permission prompt) · 12s" and "delegando a subagentes · 3m" are cut before the reason or peer and the time-in-state.
- Why it matters: spec section 6.1 lists `waitingFor` or the peer plus the time in state as tooltip content, and those are exactly the parts lost. This is the deferred Task 12 minor, promoted.
- Fix: widen the window to `max(frame*scale, ~220px)` and centre the canvas, or wrap the text (`white-space:normal`) and grow the tooltip area. Shortening the labels alone is not enough.

### Minor (Nice to Have)

- **M1.** `commands.rs:72-87`: `set_pack` sends the new manifest but never resizes existing windows. A pack with a different `frameSize`/`scale` renders clipped until the session restarts. Call `set_size` on each `sprite-*` window.
- **M2.** `manager.rs:11` and `ui/sprite/main.ts:72`: the 56 px transparent strip under each sprite blocks clicks to whatever is beneath it and is not draggable. The spec says the whole surface is a drag region. Make the whole body draggable, and toggle `set_ignore_cursor_events` or shrink the window when the tooltip is hidden.
- **M3.** `config.rs:21-26` and `commands.rs:52`: a corrupt or hand-edited config.json silently generates a new token. Installed hooks keep the old token and get 401 on every call, but `is_installed` matches by URL only and still reports "instalados". Compare the Authorization header too, and show "token desactualizado — reconectar".
- **M4.** `positions.rs:19-25`: the write is not atomic, and stale provisional keys are never pruned (see I1). Use temp file plus rename, like the installer.
- **M5.** `store.rs:84-94`: `mark_missed` ignores recent hooks. A session that sends hooks but is not listed by `claude agents` (headless `claude -p`, if not listed) flaps between Gone and Working every 5 s and may get its window recreated. Skip `mark_missed` while `last_hook_at` is under 5 s old.
- **M6.** `store.rs:55-65`: a late hook for an already-swept session (for example a duplicated SessionEnd) re-creates the agent and opens a window. The C3 tombstone covers this.
- **M7.** `hooks_server.rs:10-15`: axum's default 2 MB `DefaultBodyLimit` returns 413 for very large `PostToolUse` payloads (big `tool_response`), which loses the transition. Raise the limit, or read only the fields needed.
- **M8.** `lib.rs:40-41` and `poller.rs:38`: two tokio runtimes. The poller uses the private `rt` via `tokio::spawn`; everything else uses Tauri's runtime. Call `tauri::async_runtime::set(rt.handle().clone())` or spawn the poller with `tauri::async_runtime::spawn`.
- **M9.** `tauri.conf.json` (`"csp": null`) and `capabilities/default.json`: sprite webviews, which render untrusted agent strings, share one capability with settings and can call `install_hooks`, `uninstall_hooks`, `set_pack` and autostart. Set a strict CSP (`default-src 'self'; img-src 'self' data:`) and split the capabilities per window.
- **M10.** `ui/public/assets-dev` (symlink) and `vite.config.ts:6`: the mock assets are copied into `ui/dist` and shipped in the bundle. The symlink also breaks on Windows checkouts without `core.symlinks`. Serve them only in dev, via a Vite dev middleware or `import.meta.env.DEV`.
- **M11.** `ui/sprite/main.ts:65`: the tooltip DOM is rebuilt on every animation frame (60 Hz). Re-render once a second, or only when the seconds value changes.
- **M12.** `packs/manifest.rs:56-75`: no validation of `fps > 0` (the animator clamps), `scale > 0` (a zero gives a 0-px window) or of `sheet` being a single path component. A pack using `../x.png` passes validation, then `get_pack_url` refuses it and the sprite renders blank, with no fallback.
- **M13.** `InstallError`/`PackError` `Display` delegates to `Debug`, so the settings `alert()` shows text like `Json(Error("expected value", line: 1, column: 3))`. Write human-readable messages.
- **M14.** README gaps:
  - No mention of `BUDDY_CLAUDE_BIN` or the GUI PATH caveat (I3).
  - No `cargo tauri build`/bundle instructions.
  - No Linux tray prerequisites (libayatana-appindicator).
  - "la posición se recuerda" is currently untrue for new sessions (I1).

## Deferred-minor triage

- T1 mobile (android/ios) icons committed: CAN WAIT. About 35 unused files; delete when convenient.
- T1 esbuild postinstall warning: CAN WAIT.
- T2 `parse_snapshot` drops undeserializable entries silently: CAN WAIT. Add `log::warn`.
- T2 wrong-typed fields drop the entry instead of Unknown: CAN WAIT.
- T2 unused Serialize derives on poll enums: CAN WAIT.
- T3 SubagentStop to 0 forces Working instead of Idle: CAN WAIT. Self-heals on the first poll outside the hook window.
- T3 lost SubagentStop leaves Delegating stuck: MUST FIX BEFORE MERGE. Promoted to I2.
- T3 `waiting_for` not cleared on Stop/poll Idle: CAN WAIT. The tooltip shows it only in `needs_input`.
- T3 SendMessage without recipient overwrites peer: CAN WAIT.
- T3 missing tests (Notification while Delegating, PermissionRequest with subagents, status None/Unknown, Background Unknown): CAN WAIT. Add them alongside the I2 fix.
- T3 poll Waiting resurrects a just-ended (Gone) session: CAN WAIT. Costs about 15 s of extra sprite life, then 2-miss gone.
- T4 PackError Display is Debug-based: CAN WAIT (see M13).
- T4 sheet reopened per animation: CAN WAIT.
- T4 no tests for FramesOutOfRange/BadPng: CAN WAIT.
- T4 sheet path not guarded against `..`: CAN WAIT. The read boundary (`get_pack_url`) now refuses it; see M12 for the UX side.
- T4 pack.json trailing newline: CAN WAIT.
- T5 (security) settings.json/backup written with default perms: ALREADY FIXED. Confirmed: temp file plus rename at 0600 (`settings_json.rs:92-99`), test `install_writes_owner_only_permissions`. The backup is a `fs::copy` of the original and keeps the original's mode, which is acceptable.
- T5 whole-group removal when a foreign hook shares our group: CAN WAIT. Buddy always writes its own group, so this only happens if the user hand-merges.
- T5 Display is Debug-based: CAN WAIT (see M13).
- T5 untested CLAUDE_CONFIG_DIR: CAN WAIT.
- T5 tmp file briefly at umask perms before chmod: CAN WAIT. It lives inside the user-owned `~/.claude`, and the window is milliseconds.
- T5 uninstall rewrites the file even when nothing was removed: CAN WAIT.
- T6 config save creates the file before chmod: CAN WAIT.
- T6 token validity is length-only: CAN WAIT.
- T6 load does not persist a new token: ALREADY HANDLED. `lib.rs:48` saves after load.
- T6 relative `.buddy` fallback: CAN WAIT.
- T7 wait error labelled Spawn: CAN WAIT.
- T7 receiver-drop shutdown detected only after a success: CAN WAIT.
- T7 dedup only against the last error: CAN WAIT.
- T7 stdin not nulled: CAN WAIT. GUI stdin is already /dev/null; still a one-liner.
- T7 `spawn_poller` untested: CAN WAIT.
- T8 non-constant-time token compare: CAN WAIT. Localhost bind by default, and the token only gates status events.
- T8 full-channel drops unlogged: CAN WAIT. Note that it feeds I2.
- T8 no tests for bind failure, full channel or oversize body: CAN WAIT. The bind failure must be surfaced, though (I5).
- T8 401 tests don't assert non-delivery: CAN WAIT.
- T9 `mark_missed` hand-builds the view with `since_secs` 0: CAN WAIT. The value is correct because `since` was just reset; only the duplication remains.
- T9 no tests for multi-host isolation, Gone not re-flipped, empty-sessionId entries: CAN WAIT.
- T10 `host:name` key collision if host contains ':': CAN WAIT.
- T10 positions write not atomic: CAN WAIT (M4).
- T10 no exact wrap test: CAN WAIT.
- T11 (security) `get_pack_url` joins caller-supplied `sheet` unvalidated: ALREADY FIXED. Confirmed `is_single_normal` plus a manifest membership check (`commands.rs:19-35`), and `set_pack` checks against `list_packs`.
- T11 two tokio runtimes: CAN WAIT (M8).
- T11 `lock().unwrap()` poisoning cascade: CAN WAIT.
- T11 bind failure not surfaced in status: MUST FIX BEFORE MERGE. Part of I5.
- T11 image/png MIME hardcoded: CAN WAIT. The manifest only allows PNG by design.
- T12 (security) `innerHTML` in tooltip: ALREADY FIXED. Confirmed: no `innerHTML`/`outerHTML`/`insertAdjacentHTML` in `ui/`. The tooltip and settings use DOM plus `textContent`.
- T12 `getContext('2d')!` assertion: ALREADY FIXED (`main.ts:15-16`).
- T12 decode errors swallowed: CAN WAIT. They are logged with `console.warn`.
- T12 large dt spin in Animator: ALREADY FIXED (dt clamped to 100 ms, `main.ts:57`).
- T12 tooltip clipped by window width: MUST FIX BEFORE MERGE. Promoted to I6.
- T13 full re-render on `agents` drops focus and closes an open dropdown: CAN WAIT.
- T13 refresh/agents race: CAN WAIT.
- T13 no `.catch` on the status poll and initial refresh: CAN WAIT.
- T13 `open_settings` swallows the build error: CAN WAIT.
- T13 tray icon as template renders the coloured icon as a silhouette: CAN WAIT. Cosmetic; verify on device, and provide a monochrome tray icon.
- T13 `alert()` in the webview: CAN WAIT.

## Declined to judge

- `waiting_peer` lingering after an agent sends a reply (not a request) and stops: the spec defines SendMessage followed by Stop as `waiting_peer`. That is a product decision, not a defect.
- Error noise or latency in Claude Code when Buddy is closed but hooks are installed: this is Claude Code's behaviour. The spec chose `timeout: 2` to limit it.
- Whether Claude Code's `http` hook type, the `PermissionRequest`/`SubagentStart` event names, and the `claude agents --json --all` field names exist exactly as written: these are external contracts the spec asserts and cannot be verified offline. C3 states its one dependency on them explicitly.
- Multi-monitor and per-monitor DPI positioning: spec section 11 puts it outside v1.
- Multi-PC (remote hooks and remote poll): outside v1. Only the preparation (`host`, token, `bind_all`) is required, and it exists.
- Click to focus the terminal, sending messages, sounds: outside v1.
- Visual quality of the generated default pack and the six `?mock=` pages: a manual visual check handed to the user (ledger Task 12/14).
- One-time backup rather than a backup before every write: ruled in the ledger (Task 5) as safer against overwriting the original with a Buddy-modified copy.
- No "status" push event to settings (polling `get_status` every 3 s instead): ruled in the ledger (Task 11).
- Cascade-index race for two agents added in one batch: a known trade-off accepted in the plan's self-review.
- Commit attribution trailer: ruled in the ledger (Task 14).
- Spanish UI strings: consistent with the spec's language.
- Port not editable in settings: spec section 6.2 only asks to show the port and token.

## Recommendations

1. Fix C1 first. It is a two-line Rust change plus a one-line UI guard. Then run the two-session manual check from spec section 10, which would have caught it.
2. Fix C2 and verify with an actual `cargo tauri build`, launching the `.app` from Finder. That single launch also exercises I3 (PATH) and I4 (Dock/Cmd-Tab).
3. Add a small seam test for the manager: abstract `emit_to`/`ensure_window` behind a trait, or at least assert in a store test that terminal background agents never produce `Added` (C3).
4. Fold I2 and I5 into one "robustness" commit: reset the subagent count, add single-instance, surface server/pack errors in `Status` and the tray.
5. Batch the CAN WAIT minors into a follow-up issue list rather than this merge.

## Assessment

**Ready to merge? With fixes.** Unit-level quality is high and the installer and security fixes check out. The three Critical findings, though, break the product with more than one session (C1), in any installed build (C2) and around background agents (C3), and they need fixing together with the six Important items before a real user runs it.
