// Generates assets/packs/default/{sheet.png,pack.json}: a glossy, translucent jelly slime
// (Rimuru-style). 80x80 frames, 8 columns x 6 rows (one row per state). Body tint per state.
import { PNG } from "pngjs";
import { writeFileSync, mkdirSync } from "node:fs";

const F = 80, COLS = 8, ROWS = 6;
const png = new PNG({ width: F * COLS, height: F * ROWS });

// ---------- primitives ----------
const px = (x, y, c) => {
  x = Math.round(x); y = Math.round(y);
  if (x < 0 || y < 0 || x >= png.width || y >= png.height) return;
  const a = c[3] ?? 255;
  if (a <= 0) return;
  const i = (png.width * y + x) << 2;
  if (a >= 255) { png.data[i] = c[0]; png.data[i + 1] = c[1]; png.data[i + 2] = c[2]; png.data[i + 3] = 255; return; }
  const da = png.data[i + 3] / 255, sa = a / 255, oa = sa + da * (1 - sa);
  for (let k = 0; k < 3; k++) png.data[i + k] = Math.round((c[k] * sa + png.data[i + k] * da * (1 - sa)) / (oa || 1));
  png.data[i + 3] = Math.round(oa * 255);
};
const rect = (x, y, w, h, c) => { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) px(x + i, y + j, c); };
const hline = (x, y, w, c) => rect(x, y, w, 1, c);
const circle = (cx, cy, r, c) => { for (let y = -r; y <= r; y++) for (let x = -r; x <= r; x++) if (x * x + y * y <= r * r) px(cx + x, cy + y, c); };
const withAlpha = (c, a) => [c[0], c[1], c[2], a];
const shade = (c, k) => [Math.max(0, Math.min(255, c[0] * k)), Math.max(0, Math.min(255, c[1] * k)), Math.max(0, Math.min(255, c[2] * k)), c[3] ?? 255];
const mix = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 255];

// Jelly blob shape: a superellipse-ish drop, wider than tall, flattened at the bottom.
// Returns true for (dx,dy) relative to the blob centre with half-width rx and half-height ry.
function inBlob(dx, dy, rx, ry, lean = 0) {
  const x = dx - lean * (dy / ry); // lean tilts the top sideways
  const ny = dy / ry, nx = x / rx;
  // top half rounder (circle), bottom half squarer (superellipse n=3) and flattened
  if (ny <= 0) return nx * nx + ny * ny <= 1;
  const n = 2.8;
  return Math.pow(Math.abs(nx), n) + Math.pow(Math.abs(ny) * 1.15, n) <= 1;
}

// ---------- palette ----------
const INK = [36, 58, 92], WHITE = [255, 255, 255], RED = [232, 64, 64], YELLOW = [255, 236, 140], SCREEN = [140, 215, 255];
const RIMURU = [159, 224, 255];
const TINT = {
  working: [110, 170, 255], idle: RIMURU, needs_input: [255, 200, 110],
  delegating: [150, 235, 200], waiting_peer: [200, 170, 255], gone: [170, 180, 195],
};
const order = ["working", "idle", "needs_input", "delegating", "waiting_peer", "gone"];

// ---------- slime ----------
// centre of the blob base at (40, 58). sx/sy = horizontal/vertical stretch factors (squash & stretch).
function slime(o, tint, { sx = 1, sy = 1, lean = 0, alpha = 230, dy = 0, cx: cxo = 0 } = {}) {
  const rx = Math.round(24 * sx), ry = Math.round(20 * sy);
  const cx = o.x + 40 + cxo, cy = o.y + 58 - ry + dy; // keep the base on the floor when stretching
  const deep = shade(tint, 0.72), light = mix(tint, WHITE, 0.35), rim = mix(tint, WHITE, 0.55);
  // ground shadow
  for (let y = -3; y <= 3; y++) for (let x = -rx - 2; x <= rx + 2; x++) if ((x * x) / ((rx + 2) * (rx + 2)) + (y * y) / 9 <= 1) px(cx + x, o.y + 60 + y, [0, 0, 0, 40]);
  // outline (1px, soft)
  for (let y = -ry - 1; y <= ry + 1; y++) for (let x = -rx - 1; x <= rx + 1; x++) {
    if (inBlob(x, y, rx, ry, lean)) continue;
    if (inBlob(x + 1, y, rx, ry, lean) || inBlob(x - 1, y, rx, ry, lean) || inBlob(x, y + 1, rx, ry, lean) || inBlob(x, y - 1, rx, ry, lean)) px(cx + x, cy + y, withAlpha(INK, Math.round(alpha * 0.9)));
  }
  // body: vertical gradient deep -> light, translucent
  for (let y = -ry; y <= ry; y++) for (let x = -rx; x <= rx; x++) {
    if (!inBlob(x, y, rx, ry, lean)) continue;
    const t = (y + ry) / (2 * ry); // 0 top, 1 bottom
    const c = mix(light, deep, Math.pow(t, 1.4));
    // inner rim light near the left/top edge
    const edge = inBlob(x - 2, y - 2, rx, ry, lean) ? 0 : 1;
    px(cx + x, cy + y, withAlpha(edge ? rim : c, alpha));
  }
  // deep core glow (slightly darker, translucent) low-centre
  for (let y = 0; y <= ry; y++) for (let x = -rx; x <= rx; x++)
    if (inBlob(x, y, rx, ry, lean) && (x * x) / (rx * rx * 0.45) + ((y - ry * 0.45) ** 2) / (ry * ry * 0.3) <= 1) px(cx + x, cy + y, withAlpha(deep, 70));
  // big glossy highlight (top-left) + two small sparkles
  const hx = cx - Math.round(rx * 0.42) + Math.round(lean * 0.6), hy = cy - Math.round(ry * 0.45);
  for (let y = -6; y <= 6; y++) for (let x = -9; x <= 9; x++) if ((x * x) / 81 + (y * y) / 36 <= 1) px(hx + x, hy + y, withAlpha(WHITE, 210));
  circle(cx + Math.round(rx * 0.45), cy - Math.round(ry * 0.25), 2, withAlpha(WHITE, 230));
  px(cx + Math.round(rx * 0.55), cy + Math.round(ry * 0.2), withAlpha(WHITE, 200));
  return { cx, cy, rx, ry };
}
function face(b, { eyes = "dot", look = 0, mouth = "smile", alpha = 255 } = {}) {
  const ink = withAlpha(INK, alpha);
  const ey = b.cy + 2, lx = b.cx - 8 + look, rx = b.cx + 8 + look;
  if (eyes === "none") return;
  if (eyes === "closed") { hline(lx - 3, ey, 6, ink); hline(rx - 3, ey, 6, ink); px(lx - 3, ey - 1, ink); px(rx + 2, ey - 1, ink); }
  else if (eyes === "wide") { circle(lx, ey, 4, ink); circle(rx, ey, 4, ink); circle(lx, ey, 3, withAlpha(WHITE, alpha)); circle(rx, ey, 3, withAlpha(WHITE, alpha)); circle(lx + 1, ey, 1, ink); circle(rx + 1, ey, 1, ink); }
  else { circle(lx, ey, 2, ink); circle(rx, ey, 2, ink); px(lx - 1, ey - 1, withAlpha(WHITE, alpha)); px(rx - 1, ey - 1, withAlpha(WHITE, alpha)); }
  const my = b.cy + 9;
  if (mouth === "smile") { px(b.cx - 3, my - 1, ink); px(b.cx - 2, my, ink); px(b.cx - 1, my, ink); px(b.cx, my, ink); px(b.cx + 1, my, ink); px(b.cx + 2, my, ink); px(b.cx + 3, my - 1, ink); }
  else if (mouth === "o") { circle(b.cx, my, 3, ink); circle(b.cx, my, 2, withAlpha(shade(RED, 0.85), alpha)); }
  else if (mouth === "flat") hline(b.cx - 3, my, 7, ink);
  else if (mouth === "wavy") { px(b.cx - 3, my, ink); px(b.cx - 2, my - 1, ink); px(b.cx - 1, my, ink); px(b.cx, my + 1, ink); px(b.cx + 1, my, ink); px(b.cx + 2, my - 1, ink); px(b.cx + 3, my, ink); }
}

// ---------- props ----------
function laptop(o, keyGlow) {
  const x = o.x + 26, y = o.y + 66;
  rect(x, y, 28, 5, INK); rect(x + 1, y + 1, 26, 3, [70, 80, 100]);
  rect(x + 3, y - 12, 22, 13, INK); rect(x + 4, y - 11, 20, 11, SCREEN);
  for (let i = 0; i < 3; i++) hline(x + 6, y - 9 + i * 3, 7 + ((i + keyGlow) % 3) * 4, shade(SCREEN, 0.6));
}
function exclaim(o, dy, alpha = 255) {
  const x = o.x + 38, y = o.y + 4 + dy;
  rect(x - 1, y - 1, 6, 15, withAlpha(INK, alpha)); rect(x, y, 4, 12, withAlpha(RED, alpha));
  rect(x - 1, y + 15, 6, 5, withAlpha(INK, alpha)); rect(x, y + 16, 4, 3, withAlpha(RED, alpha));
}
function zee(x, y, alpha, big = false) {
  const a = withAlpha(INK, alpha), w = big ? 7 : 5;
  hline(x, y, w, a); for (let i = 1; i < w - 1; i++) px(x + w - 1 - i, y + i, a); hline(x, y + w - 1, w, a);
}
function clock(x, y, frame) {
  circle(x, y, 9, INK); circle(x, y, 8, WHITE);
  px(x, y - 5, INK); px(x, y + 5, INK); px(x - 5, y, INK); px(x + 5, y, INK);
  const a = (frame / 8) * Math.PI * 2;
  for (let t = 1; t <= 6; t++) px(x + Math.round(Math.sin(a) * t), y - Math.round(Math.cos(a) * t), INK);
  px(x - 2, y, INK); px(x - 3, y, INK);
}
function dots(x, y, n) { for (let i = 0; i < n; i++) circle(x + i * 4, y, 1, INK); }
function sparkle(x, y, c = YELLOW) { px(x, y, c); px(x - 1, y, c); px(x + 1, y, c); px(x, y - 1, c); px(x, y + 1, c); px(x - 2, y, withAlpha(c, 120)); px(x + 2, y, withAlpha(c, 120)); px(x, y - 2, withAlpha(c, 120)); px(x, y + 2, withAlpha(c, 120)); }
function miniSlime(o, x, y, tint, bob) {
  const rx = 8, ry = 7, cx = x, cy = y - bob;
  const deep = shade(tint, 0.72), light = mix(tint, WHITE, 0.35);
  for (let yy = -ry - 1; yy <= ry + 1; yy++) for (let xx = -rx - 1; xx <= rx + 1; xx++) {
    if (inBlob(xx, yy, rx, ry)) continue;
    if (inBlob(xx + 1, yy, rx, ry) || inBlob(xx - 1, yy, rx, ry) || inBlob(xx, yy + 1, rx, ry) || inBlob(xx, yy - 1, rx, ry)) px(cx + xx, cy + yy, withAlpha(INK, 200));
  }
  for (let yy = -ry; yy <= ry; yy++) for (let xx = -rx; xx <= rx; xx++) if (inBlob(xx, yy, rx, ry)) px(cx + xx, cy + yy, withAlpha(mix(light, deep, (yy + ry) / (2 * ry)), 230));
  circle(cx - 3, cy - 3, 2, withAlpha(WHITE, 210));
  px(cx - 3, cy + 1, INK); px(cx + 3, cy + 1, INK);
}

// ---------- animations ----------
const anims = {
  working(o, f) {
    const wob = [0, 0.03, 0.05, 0.03, 0, -0.03, -0.05, -0.03][f];
    const b = slime(o, TINT.working, { sx: 1 + wob, sy: 1 - wob, lean: 0 });
    face(b, { eyes: "dot", look: 0, mouth: "smile" });
    laptop(o, f);
    if (f % 4 === 0) sparkle(o.x + 62, o.y + 44); if (f % 4 === 2) sparkle(o.x + 16, o.y + 40);
  },
  idle(o, f) {
    const t = Math.sin((f / 6) * Math.PI * 2);
    const b = slime(o, TINT.idle, { sx: 1 + t * 0.04, sy: 1 - t * 0.05 });
    face(b, { eyes: "none" });
    const z = f / 6;
    zee(o.x + 58, o.y + 18 - Math.round(z * 10), 255 - Math.round(z * 160), true);
    if (f >= 2) zee(o.x + 64, o.y + 26 - (f - 2) * 2, 200);
  },
  needs_input(o, f) {
    const stretch = [0, 0.18, 0.3, 0.3, 0.18, 0][f % 6];
    const b = slime(o, TINT.needs_input, { sx: 1 - stretch * 0.4, sy: 1 + stretch });
    face(b, { eyes: "wide", mouth: "o" });
    exclaim(o, -Math.round(stretch * 20) + (f % 2));
  },
  delegating(o, f) {
    const spread = [0, 2, 5, 8, 10, 10, 8, 4][f];
    const b = slime(o, TINT.delegating, { sx: 1 - spread * 0.015, sy: 1 + spread * 0.01, lean: f < 4 ? -2 : 2 });
    face(b, { eyes: "dot", look: f < 4 ? -2 : 2, mouth: "smile" });
    const bob = [0, 1, 3, 2, 0, 1, 3, 2][f];
    miniSlime(o, o.x + 12 - Math.round(spread * 0.4), o.y + 66, TINT.delegating, bob);
    miniSlime(o, o.x + 68 + Math.round(spread * 0.4), o.y + 66, TINT.delegating, 3 - bob);
  },
  waiting_peer(o, f) {
    const lean = [-4, -4, -2, 0, 4, 4, 2, 0][f];
    const b = slime(o, TINT.waiting_peer, { lean, sx: 1, sy: 1 });
    face(b, { eyes: "dot", look: lean < 0 ? -3 : lean > 0 ? 3 : 0, mouth: "wavy" });
    clock(o.x + 66, o.y + 14, f);
    dots(o.x + 8, o.y + 14, (f % 3) + 1);
  },
  gone(o, f) {
    const t = f / 7;
    const alpha = Math.round(230 * (1 - t * 0.9));
    const b = slime(o, TINT.gone, { sx: 1 + t * 0.5, sy: 1 - t * 0.85, alpha });
    if (f < 3) face(b, { eyes: "closed", mouth: "flat", alpha });
    for (let i = 0; i < 3; i++) {
      const pa = Math.round(220 * (1 - t));
      circle(o.x + 22 + i * 18, o.y + 40 - f * 4 - i * 3, 1, withAlpha(mix(TINT.gone, WHITE, 0.5), pa));
    }
  },
};
const frames = { working: 8, idle: 6, needs_input: 6, delegating: 8, waiting_peer: 8, gone: 8 };
const fps = { working: 10, idle: 3, needs_input: 8, delegating: 8, waiting_peer: 4, gone: 10 };

order.forEach((state, row) => {
  for (let col = 0; col < frames[state]; col++) anims[state]({ x: col * F, y: row * F }, col);
});

mkdirSync("assets/packs/default", { recursive: true });
writeFileSync("assets/packs/default/sheet.png", PNG.sync.write(png));
const manifest = { name: "Slime", frameSize: [F, F], scale: 2, animations: {} };
order.forEach((state, row) => {
  manifest.animations[state] = { sheet: "sheet.png", row, frames: frames[state], fps: fps[state], ...(state === "gone" ? { loop: false } : {}) };
});
writeFileSync("assets/packs/default/pack.json", JSON.stringify(manifest, null, 2) + "\n");
console.log(`slime (default) pack written (${png.width}x${png.height})`);
