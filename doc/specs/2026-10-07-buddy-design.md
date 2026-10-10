# Buddy — Desktop pet para sesiones de Claude Code

Fecha: 2026-10-07
Estado: borrador para revisión

## 1. Objetivo

Buddy es una app de escritorio con ventanas transparentes que muestra un sprite animado
por cada sesión de Claude Code activa en la máquina. El sprite cambia de animación según
el estado del agente para que, de un vistazo y sin cambiar de ventana, se sepa qué agente
está trabajando, cuál está inactivo, cuál necesita atención del usuario y cuál espera a
otro agente.

**Para quién:** el propio usuario, con varias sesiones de Claude Code abiertas a la vez.

**Éxito:** mirar la pantalla y saber qué agente necesita atención sin tocar nada.

## 2. Decisiones tomadas

| Tema | Decisión |
|---|---|
| Stack | Tauri 2 (Rust + WebView), multiplataforma (macOS, Windows, Linux). |
| Multi-PC | Diseñado para ello (campo `host`, token, `bind_all`), no implementado en v1. |
| Relación sprite/sesión | Un sprite por sesión, pasivo: anima según estado, click muestra tooltip. |
| Assets | Packs de sprites por manifiesto JSON. Pack placeholder `default` incluido. |
| Ventanas | Una ventana Tauri transparente por sprite, arrastrable, posición persistida. |
| Fuente de estado | Híbrido: hooks HTTP de Claude Code (push) + poll de `claude agents --json --all`. |

## 3. Arquitectura

```
hook http ──► hooks_server ─┐
                            ├─► AgentEvent ─► state ─► cambio ─► windows ─► UI (sprite)
claude agents --json ─► poller ┘
```

### 3.1 Core (Rust, proceso Tauri)

- **collector**: normaliza ambas fuentes a `AgentEvent`.
  - `hooks_server`: HTTP en `127.0.0.1:7433` (configurable). `POST /hook` con
    `Authorization: Bearer <token>`. Responde `200` vacío en < 50 ms; el procesamiento
    va a una cola interna. `GET /health` para que el instalador compruebe la app.
  - `poller`: cada 2.5 s ejecuta `claude agents --json --all` (timeout 2 s), parsea y emite
    un evento `Snapshot` con la lista completa.
  - `event.rs`: traducción pura de JSON de hook y de poll a `AgentEvent`.
- **state**: máquina de estados por agente (sección 4). Única fuente de verdad.
- **windows**: observa `state`. Crea una ventana por agente nuevo, cierra las de agentes
  `gone` tras 10 s de gracia, persiste posiciones en el directorio de configuración de la
  app (`positions.json`) indexadas por `host + name`.
- **installer**: añade o quita el bloque de hooks en `~/.claude/settings.json` (sección 6).
- **packs**: parseo y validación de manifiestos de sprites (sección 5).
- **config**: puerto, token, pack activo, `bind_all` (false por defecto), autoarranque.

### 3.2 UI (web, Vite + TypeScript, sin framework pesado)

- **sprite**: una ventana por agente. Recibe `AgentState` por evento Tauri, reproduce la
  animación del pack. Canvas 2D con `imageSmoothingEnabled = false`.
- **settings**: ventana desde icono de bandeja.
- **shared**: tipos compartidos con el core.

### 3.3 Tipos principales

```rust
struct AgentEvent { host: String, session_id: String, kind: EventKind, at: Instant }

enum EventKind {
    Hook { event: String, tool_name: Option<String>, notification_type: Option<String>,
           payload: serde_json::Value },
    Snapshot(Vec<PolledAgent>),
}

struct PolledAgent { session_id, name, cwd, kind: Interactive | Background,
                     status: Option<Busy | Waiting | Idle>, waiting_for: Option<String>,
                     state: Option<Working | Blocked | Done | Failed | Stopped> }

struct AgentState { id: AgentId /* host + session_id */, name: String, cwd: PathBuf,
                    visible: Visible, waiting_for: Option<String>, peer: Option<String>,
                    subagents: u32, since: Instant, last_hook_at: Option<Instant>,
                    last_seen: Instant }

enum Visible { Working, Idle, NeedsInput, Delegating, WaitingPeer, Gone }
```

## 4. Máquina de estados

Prioridad entre fuentes: un hook recibido hace menos de 5 s gana sobre el poll; pasado ese
tiempo manda el poll. `delegating` tiene prioridad sobre `working` mientras `subagents > 0`.
`waiting_peer` es inferido; si el poll reporta `waiting` con `waitingFor`, ese dato gana.

| Visible | Entra con | Sale con |
|---|---|---|
| `working` | hook `UserPromptSubmit`, `PreToolUse`, `PostToolUse` (tool ≠ SendMessage); poll `status: busy` o `state: working` | cualquier otro evento |
| `idle` | hook `Stop`; poll `status: idle` | `UserPromptSubmit` o `busy` |
| `needs_input` | hook `PermissionRequest`; `Notification` con `permission_prompt`, `elicitation_dialog` o `idle_prompt`; poll `status: waiting` (guarda `waitingFor`); background `state: blocked` | `UserPromptSubmit`, `PostToolUse`, poll `busy` |
| `delegating` | `SubagentStart` (subagents += 1) | `SubagentStop` deja subagents en 0 |
| `waiting_peer` | `PostToolUse` con `tool_name == "SendMessage"` (guarda destinatario) seguido de `Stop` o poll `idle` | `UserPromptSubmit` o poll `busy` |
| `gone` | `SessionEnd`; ausente del poll en 2 ciclos consecutivos; background `done`, `failed`, `stopped` | reaparece el mismo `session_id` dentro del tiempo de gracia (se cancela el cierre) |

Reglas adicionales:
- Un agente que llega por hook antes de aparecer en el poll se crea con nombre provisional
  `session_id[:8]` y `cwd` del hook; el poll lo completa con `name`.
- `since` se reinicia solo cuando cambia `visible`.
- Un evento malformado se descarta y se registra; nunca cambia el estado.

## 5. Packs de sprites

Carpeta `assets/packs/<nombre>/` con `pack.json` y uno o más PNG (spritesheets en
cuadrícula, una fila por animación).

```json
{
  "name": "default",
  "frameSize": [64, 64],
  "scale": 2,
  "animations": {
    "working":      { "sheet": "sheet.png", "row": 0, "frames": 6, "fps": 10 },
    "idle":         { "sheet": "sheet.png", "row": 1, "frames": 4, "fps": 4 },
    "needs_input":  { "sheet": "sheet.png", "row": 2, "frames": 4, "fps": 6 },
    "delegating":   { "sheet": "sheet.png", "row": 3, "frames": 6, "fps": 8 },
    "waiting_peer": { "sheet": "sheet.png", "row": 4, "frames": 4, "fps": 4 },
    "gone":         { "sheet": "sheet.png", "row": 5, "frames": 6, "fps": 12, "loop": false }
  }
}
```

- Las seis animaciones son obligatorias. `loop` es `true` por defecto.
- El core valida el pack al cargar (campos presentes, PNG existentes, dimensiones
  múltiplo de `frameSize`). Si falla, usa `default` y avisa en la bandeja.
- El pack `default` lo generamos nosotros: pixel art simple, distinguible por estado
  (trabajando: tecleando; idle: durmiendo; needs_input: signo de exclamación; delegating:
  mini-clones; waiting_peer: mirando un reloj; gone: desvanecimiento).

## 6. Ventanas y UI

### 6.1 Ventana de sprite
- Tamaño `frameSize × scale` más margen para el tooltip. Transparente, sin decoraciones,
  always-on-top, oculta de barra de tareas y alt-tab.
- Toda la superficie es región de arrastre. Al soltar, la UI envía la posición al core.
- Click sin arrastre alterna el tooltip: nombre, `cwd` abreviado, estado, `waitingFor` o
  destinatario del peer, y tiempo en ese estado.
- Posición inicial: la última conocida para ese `host + name`; si no hay, en cascada desde
  la esquina inferior derecha del monitor principal.
- Modo `?mock=<estado>` para revisión visual sin Claude Code.

### 6.2 Ventana de ajustes (bandeja)
- Conectar/desconectar hooks, con indicador de instalados.
- Selector de pack.
- Lista de agentes activos con estado.
- Puerto y token del servidor.
- Arrancar al iniciar sesión.
- Icono de advertencia si collector o poller reportan errores.

## 7. Integración con Claude Code

### 7.1 Hooks que instala el instalador

Eventos: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
`Stop`, `PermissionRequest`, `Notification`, `SubagentStart`, `SubagentStop`.

Entrada por evento:

```json
{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:7433/hook",
              "headers": { "Authorization": "Bearer <token>" }, "timeout": 2 }] }
```

### 7.2 Reglas del instalador
- Copia de seguridad `settings.json.buddy-backup` antes de escribir.
- Fusiona: añade nuestra entrada al array del evento; nunca borra entradas ajenas.
- Nuestras entradas se identifican por la URL; desinstalar es filtrar por esa URL.
- `timeout` corto para no frenar a Claude Code si la app no está abierta.

### 7.3 Poller
- Si `claude` no está en el PATH o la salida no parsea: registra una vez, muestra
  advertencia, reintenta el siguiente ciclo. La app sigue con hooks solamente.

### 7.4 Preparación multi-PC (no implementado en v1)
- `host` se rellena con el hostname local.
- `bind_all` apagado por defecto; al encenderlo el servidor escucha en `0.0.0.0`.
- El token ya viaja en cabecera. Un PC remoto apuntaría sus hooks a esta IP.
- El poll remoto requeriría un reporter en la otra máquina (fuera de alcance).

## 8. Manejo de errores

- Fallos en collector o poller: log + icono de advertencia en bandeja. Nunca cierran la app.
- Evento malformado: se descarta.
- Pack inválido: se usa `default`.
- `settings.json` inválido al instalar: no se escribe, se muestra el error.

## 9. Estructura del proyecto

```
buddy/
  src-tauri/
    src/
      main.rs
      collector/   hooks_server.rs, poller.rs, event.rs
      state/       agent_state.rs, machine.rs
      windows/     manager.rs, positions.rs
      installer/   settings_json.rs
      packs/       manifest.rs
      config.rs
    tauri.conf.json
  ui/
    sprite/
    settings/
    shared/
  assets/packs/default/
  doc/specs/
  releases/
```

La carpeta `api` existente no se usa y se elimina.

## 10. Pruebas

- Rust unitarias: `state::machine` (tabla `(estado, evento) → estado`), `packs::manifest`
  (válidos e inválidos), `installer` (fusión y desinstalación sobre JSON de ejemplo),
  `collector::event` (JSON de hook y de poll → `AgentEvent`).
- Integración: `hooks_server` en puerto libre, enviar un hook grabado, verificar cambio en
  `state`.
- UI: revisión visual manual con `?mock`.
- Manual final: dos sesiones de Claude Code, un `SendMessage` entre ellas, verificar los
  seis estados.

## 11. Alcance

**v1:** todo lo anterior.

**Fuera de v1:** multi-PC, click para enfocar el terminal, enviar mensajes desde el sprite,
sonidos, posiciones por monitor en multi-monitor.
