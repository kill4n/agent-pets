# Buddy

Desktop pet para Claude Code: un sprite flotante por sesión, animado según su estado
(trabajando, idle, necesita respuesta, delegando, esperando a otro agente, terminó).

## Requisitos
- Rust 1.93+, Node 20+, Claude Code 2.1.224+ (`claude agents --json`).

## Desarrollo
```bash
npm install
npm run gen:pack        # regenera assets/packs/default
cargo tauri dev
```
Revisión visual sin Claude Code: `npm run dev` y abre
`http://localhost:1420/sprite/index.html?mock=working` (o idle, needs_input, delegating, waiting_peer, gone).

## Uso
1. Abre Buddy (icono en la bandeja) → Ajustes → **Conectar con Claude Code**.
   Esto añade hooks `http` a `~/.claude/settings.json` (copia previa en `settings.json.buddy-backup`).
2. Cada sesión de Claude Code aparece como un sprite. Arrástralo donde quieras; la posición se recuerda.
3. Click en el sprite: nombre, carpeta, estado.

## Packs de sprites
`assets/packs/<nombre>/pack.json` + PNG. Seis animaciones obligatorias: `working`, `idle`,
`needs_input`, `delegating`, `waiting_peer`, `gone`. Ver `assets/packs/default/pack.json`.

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
