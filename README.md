# Buddy

A desktop pet for Claude Code: one floating sprite per session, animated according to its state
(working, idle, needs input, delegating, waiting for another agent, finished).

## Requirements
- Rust 1.93+, Node 20+, Claude Code 2.1.224+ (`claude agents --json`).

## Development
```bash
npm install
npm run gen:pack        # regenerates assets/packs/default (Slime) and assets/packs/fox (Fox)
cargo tauri dev
```
Visual check without Claude Code: run `npm run dev` and open
`http://localhost:1420/sprite/index.html?mock=working` (or idle, needs_input, delegating, waiting_peer, gone).

## Usage
1. Open Buddy (tray icon) → Settings → **Connect to Claude Code**.
   This adds `http` hooks to `~/.claude/settings.json` (the previous file is backed up as `settings.json.buddy-backup`).
2. Each Claude Code session appears as a sprite. Drag it wherever you like; its position is remembered.
3. Click a sprite to see its name, folder and state.
4. If Buddy cannot find `claude` (when launched from Finder, the Dock or at login, `PATH` is minimal),
   tell it where the binary is. Lookup order: `claude_path` in
   `~/Library/Application Support/dev.kill4n.codepets/config.json` (e.g. `"claude_path": "/Users/me/.local/bin/claude"`),
   the `BUDDY_CLAUDE_BIN` environment variable, `command -v claude` in your login shell, and the usual locations
   (`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.claude/local`, `~/.npm-global/bin`).
   Warnings (poll, hooks server, pack) appear in Settings and in the tray icon tooltip.

## Sprite packs
`assets/packs/<name>/pack.json` + PNGs. Six required animations: `working`, `idle`,
`needs_input`, `delegating`, `waiting_peer`, `gone`. See `assets/packs/default/pack.json`.
Bundled packs: `default` (Slime) and `fox` (Fox). Switch them in Settings; the manifest's `name`
field is what the dropdown shows.

## Releases
Every `vX.Y.Z` tag triggers the `release.yml` workflow, which builds for macOS (Apple Silicon and Intel),
Windows and Linux and publishes a GitHub Release with the installers. To sign and notarize on macOS,
add the secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` to the repository; without them the builds are unsigned.

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## How it works
- Claude Code HTTP hooks → `POST 127.0.0.1:7433/hook` (immediate push).
- Poll of `claude agents --json --all` every 2.5 s (reconciliation).
- Spec: `doc/specs/2026-10-07-buddy-design.md`.

## Manual checks
Things to verify by eye (not automatable):
- All six `?mock=` pages (working, idle, needs_input, delegating, waiting_peer, gone) look right and animate correctly.
- Dragging a sprite and its position being kept after quitting and relaunching Buddy.
- Tooltip: name, folder, state, `(permission prompt)` and `→ <peer>`.
- Tray menu.
- Settings → **Connect to Claude Code** (hooks in `~/.claude/settings.json`) and a test with two real sessions.
- Autostart.

## License

MIT. See [LICENSE](LICENSE).
