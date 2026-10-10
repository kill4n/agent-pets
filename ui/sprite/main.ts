// ui/sprite/main.ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AgentView, Manifest } from "../shared/types";
import { Animator } from "./animator";
import { render as renderTip } from "./tooltip";

const params = new URLSearchParams(location.search);
const key = params.get("key") ?? "";
const mock = params.get("mock");

const canvas = document.getElementById("c") as HTMLCanvasElement;
const tip = document.getElementById("tip") as HTMLDivElement;
const ctx2d = canvas.getContext("2d");
if (!ctx2d) throw new Error("2d context unavailable");
const ctx: CanvasRenderingContext2D = ctx2d;
ctx.imageSmoothingEnabled = false;

let manifest: Manifest;
let sheets = new Map<string, HTMLImageElement>();
let animator: Animator;
let view: AgentView | null = null;
let viewAt = performance.now();
let tipVisible = false;

let loadChain: Promise<void> = Promise.resolve();
function loadManifest(m: Manifest): Promise<void> {
  loadChain = loadChain.then(() => doLoadManifest(m)).catch((e) => console.warn("loadManifest failed", e));
  return loadChain;
}

async function doLoadManifest(m: Manifest) {
  manifest = m;
  const [fw, fh] = m.frameSize;
  canvas.width = fw; canvas.height = fh;
  canvas.style.width = `${fw * m.scale}px`; canvas.style.height = `${fh * m.scale}px`;
  // the speech bubble sits right above the sprite; tell CSS how tall the sprite is
  document.documentElement.style.setProperty("--sprite-h", `${fh * m.scale}px`);
  sheets = new Map();
  for (const a of Object.values(m.animations)) {
    if (sheets.has(a.sheet)) continue;
    const img = new Image();
    img.src = mock ? `/assets-dev/${a.sheet}` : await invoke<string>("get_pack_url", { sheet: a.sheet });
    await img.decode().catch((e) => console.warn("sheet decode failed", a.sheet, e));
    sheets.set(a.sheet, img);
  }
  animator = new Animator(m, view?.visible ?? "idle");
}

function setView(v: AgentView) {
  view = v; viewAt = performance.now();
  animator?.play(v.visible);
  if (tipVisible) renderTip(tip, v, 0);
}

let last = performance.now();
function frame(now: number) {
  const dt = Math.min(now - last, 100); last = now;
  if (animator && manifest) {
    const { sheet, sx, sy } = animator.tick(dt);
    const img = sheets.get(sheet);
    const [fw, fh] = manifest.frameSize;
    ctx.clearRect(0, 0, fw, fh);
    if (img?.complete) ctx.drawImage(img, sx, sy, fw, fh, 0, 0, fw, fh);
  }
  if (tipVisible && view) renderTip(tip, view, Math.floor((now - viewAt) / 1000));
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// drag vs click: a mousedown followed by movement starts an OS drag; a plain click toggles the tooltip
let downAt: { x: number; y: number } | null = null;
canvas.addEventListener("mousedown", (e) => { if (e.button === 0) downAt = { x: e.screenX, y: e.screenY }; });
canvas.addEventListener("mousemove", (e) => {
  if (!downAt) return;
  if (Math.hypot(e.screenX - downAt.x, e.screenY - downAt.y) > 4) {
    downAt = null;
    if (!mock) getCurrentWindow().startDragging().catch((err) => console.warn("startDragging failed", err));
  }
});
canvas.addEventListener("mouseup", () => {
  if (downAt) { tipVisible = !tipVisible; tip.classList.toggle("show", tipVisible); if (view) renderTip(tip, view, 0); }
  downAt = null;
});

async function main() {
  if (mock) {
    const m: Manifest = await (await fetch("/assets-dev/pack.json")).json();
    for (const a of Object.values(m.animations)) a.loop ??= true; // Rust applies this serde default; raw pack.json omits it
    await loadManifest(m);
    setView({ key: "mock", host: "local", session_id: "mock0000", name: "mock-agent", cwd: "/Users/me/proj",
      visible: mock as AgentView["visible"], waiting_for: mock === "needs_input" ? "permission prompt" : null,
      peer: mock === "waiting_peer" ? "buddy-2" : null, since_secs: 0 });
    return;
  }
  let gotEvent = false;
  await listen<AgentView>("agent-state", (e) => {
    if (e.payload.key !== key) return; // defence in depth: only this window's agent
    gotEvent = true; setView(e.payload);
  });
  await listen<Manifest>("manifest", (e) => { loadManifest(e.payload); });
  const w = getCurrentWindow();
  let moveTimer: number | undefined;
  await w.onMoved(() => {
    clearTimeout(moveTimer);
    moveTimer = window.setTimeout(async () => {
      try {
        const p = await w.outerPosition();
        const f = await w.scaleFactor();
        await invoke("report_position", { key, x: Math.round(p.x / f), y: Math.round(p.y / f) });
      } catch (err) { console.warn("report_position failed", err); }
    }, 300);
  });
  // null when no valid pack is installed: stay blank until set_pack emits "manifest"
  const m = await invoke<Manifest | null>("get_manifest");
  if (m) await loadManifest(m);
  const agents = await invoke<AgentView[]>("list_agents");
  const mine = agents.find((a) => a.key === key);
  if (mine && !gotEvent) setView(mine);
}
main();
