# Buddy

Desktop pet para Claude Code: un sprite flotante por sesión, animado según su estado
(trabajando, idle, necesita respuesta, delegando, esperando a otro agente, terminó).

## Requisitos
- Rust 1.93+, Node 20+, Claude Code 2.1.224+ (`claude agents --json`).

## Desarrollo
```bash
npm install
npm run gen:pack        # regenera assets/packs/default (Slime) y assets/packs/fox (Zorro)
cargo tauri dev
```
Revisión visual sin Claude Code: `npm run dev` y abre
`http://localhost:1420/sprite/index.html?mock=working` (o idle, needs_input, delegating, waiting_peer, gone).

## Uso
1. Abre Buddy (icono en la bandeja) → Ajustes → **Conectar con Claude Code**.
   Esto añade hooks `http` a `~/.claude/settings.json` (copia previa en `settings.json.buddy-backup`).
2. Cada sesión de Claude Code aparece como un sprite. Arrástralo donde quieras; la posición se recuerda.
3. Click en el sprite: nombre, carpeta, estado.
4. Si Buddy no encuentra `claude` (al abrirlo desde Finder, el Dock o al iniciar sesión el `PATH` es mínimo),
   indica la ruta del binario. Orden de búsqueda: `claude_path` en
   `~/Library/Application Support/dev.kill4n.codepets/config.json` (p. ej. `"claude_path": "/Users/yo/.local/bin/claude"`),
   la variable de entorno `BUDDY_CLAUDE_BIN`, `command -v claude` en tu shell de login y las rutas habituales
   (`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.claude/local`, `~/.npm-global/bin`).
   Los avisos (poll, servidor de hooks, pack) aparecen en Ajustes y en el tooltip del icono de la bandeja.

## Packs de sprites
`assets/packs/<nombre>/pack.json` + PNG. Seis animaciones obligatorias: `working`, `idle`,
`needs_input`, `delegating`, `waiting_peer`, `gone`. Ver `assets/packs/default/pack.json`.
Packs incluidos: `default` (Slime) y `fox` (Zorro). Se cambian desde Ajustes; el campo `name`
del manifiesto es lo que muestra el desplegable.

## Releases
Cada tag `vX.Y.Z` dispara el workflow `release.yml`, que compila para macOS (Apple Silicon e Intel),
Windows y Linux y publica una GitHub Release con los instaladores. Para firmar y notarizar en macOS
añade los secretos `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_ID`, `APPLE_PASSWORD` y `APPLE_TEAM_ID` al repositorio; sin ellos las builds salen sin firmar.

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## Cómo funciona
- Hooks HTTP de Claude Code → `POST 127.0.0.1:7433/hook` (push inmediato).
- Poll de `claude agents --json --all` cada 2.5 s (reconciliación).
- Spec: `doc/specs/2026-10-07-buddy-design.md`.

## Comprobación visual pendiente
Pendiente de verificar a ojo (no automatizable):
- Las seis páginas `?mock=` (working, idle, needs_input, delegating, waiting_peer, gone) se ven y animan bien.
- Arrastrar un sprite y que su posición se conserve tras cerrar y relanzar Buddy.
- Tooltip: nombre, carpeta, estado, `(permission prompt)` y `→ <par>`.
- Menú de la bandeja.
- Ajustes → **Conectar con Claude Code** (hooks en `~/.claude/settings.json`) y la prueba con dos sesiones reales.
- Autostart.
