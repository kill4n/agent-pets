// Generates assets/packs/default/{sheet.png,pack.json}: a round "slime with arms" mascot
// drawn with pixel-art primitives. 64x64 frames, 8 columns x 6 rows (one row per state).
import { PNG } from "pngjs";
import { writeFileSync, mkdirSync } from "node:fs";

const F = 64, COLS = 8, ROWS = 6;
const png = new PNG({ width: F * COLS, height: F * ROWS });

// ---------- primitives ----------
const px = (x, y, c) => {
  x = Math.round(x); y = Math.round(y);
  if (x < 0 || y < 0 || x >= png.width || y >= png.height) return;
  const a = c[3] ?? 255;
  if (a <= 0) return;
  const i = (png.width * y + x) << 2;
  if (a >= 255) { png.data[i] = c[0]; png.data[i + 1] = c[1]; png.data[i + 2] = c[2]; png.data[i + 3] = 255; return; }
  // simple source-over blend
  const da = png.data[i + 3] / 255, sa = a / 255, oa = sa + da * (1 - sa);
  for (let k = 0; k < 3; k++) png.data[i + k] = Math.round((c[k] * sa + png.data[i + k] * da * (1 - sa)) / (oa || 1));
  png.data[i + 3] = Math.round(oa * 255);
};
const rect = (x, y, w, h, c) => { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) px(x + i, y + j, c); };
const hline = (x, y, w, c) => rect(x, y, w, 1, c);
const inEllipse = (dx, dy, rx, ry) => (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1;
const ellipse = (cx, cy, rx, ry, c) => {
  for (let y = -ry; y <= ry; y++) for (let x = -rx; x <= rx; x++) if (inEllipse(x, y, rx, ry)) px(cx + x, cy + y, c);
};
const ellipseOutline = (cx, cy, rx, ry, c) => {
  for (let y = -ry - 1; y <= ry + 1; y++) for (let x = -rx - 1; x <= rx + 1; x++) {
    if (inEllipse(x, y, rx, ry)) continue;
    const touches = inEllipse(x + 1, y, rx, ry) || inEllipse(x - 1, y, rx, ry) || inEllipse(x, y + 1, rx, ry) || inEllipse(x, y - 1, rx, ry);
    if (touches) px(cx + x, cy + y, c);
  }
};
const circle = (cx, cy, r, c) => ellipse(cx, cy, r, r, c);
const withAlpha = (c, a) => [c[0], c[1], c[2], a];
const shade = (c, k) => [Math.max(0, Math.min(255, c[0] * k)), Math.max(0, Math.min(255, c[1] * k)), Math.max(0, Math.min(255, c[2] * k)), c[3] ?? 255];

// ---------- palette ----------
const INK = [30, 28, 40], WHITE = [250, 250, 252], RED = [232, 64, 64], YELLOW = [255, 220, 90], SCREEN = [120, 200, 255];
const BASE = {
  working: [86, 160, 255], idle: [150, 152, 170], needs_input: [255, 160, 56],
  delegating: [120, 210, 130], waiting_peer: [190, 140, 255], gone: [110, 110, 120],
};
const order = ["working", "idle", "needs_input", "delegating", "waiting_peer", "gone"];

// ---------- mascot parts ----------
// Body centre is (32, 40) in frame space; body radius 17x14. `o` = frame origin, `dy` = vertical bob, `sq` = squash.
function body(o, base, { dy = 0, sq = 0, alpha = 255, scale = 1 } = {}) {
  const cx = o.x + 32, cy = o.y + 40 + dy;
  const rx = Math.round(17 * scale + sq), ry = Math.round(14 * scale - sq);
  const a = (c) => withAlpha(c, alpha);
  ellipseOutline(cx, cy, rx, ry, a(INK));
  ellipse(cx, cy, rx, ry, a(base));
  // bottom shadow band
  for (let y = Math.round(ry * 0.45); y <= ry; y++) for (let x = -rx; x <= rx; x++) if (inEllipse(x, y, rx, ry)) px(cx + x, cy + y, a(shade(base, 0.78)));
  // top highlight
  ellipse(cx - Math.round(rx * 0.35), cy - Math.round(ry * 0.45), Math.max(2, Math.round(rx * 0.25)), Math.max(1, Math.round(ry * 0.15)), a(shade(base, 1.25)));
  return { cx, cy, rx, ry };
}
function eyes(b, { open = true, look = 0, wide = false, alpha = 255 } = {}) {
  const a = (c) => withAlpha(c, alpha);
  const ey = b.cy - 3, lx = b.cx - 6, rxp = b.cx + 6;
  if (!open) { hline(lx - 3, ey, 6, a(INK)); hline(rxp - 3, ey, 6, a(INK)); return; }
  const r = wide ? 4 : 3;
  circle(lx, ey, r, a(INK)); circle(rxp, ey, r, a(INK));
  circle(lx, ey, r - 1, a(WHITE)); circle(rxp, ey, r - 1, a(WHITE));
  px(lx + look, ey + (wide ? 0 : 1), a(INK)); px(rxp + look, ey + (wide ? 0 : 1), a(INK));
  if (wide) { px(lx + look + 1, ey, a(INK)); px(rxp + look + 1, ey, a(INK)); }
}
function mouth(b, kind = "smile", alpha = 255) {
  const a = withAlpha(INK, alpha);
  const my = b.cy + 4;
  if (kind === "smile") { hline(b.cx - 2, my, 5, a); px(b.cx - 3, my - 1, a); px(b.cx + 3, my - 1, a); }
  else if (kind === "o") { circle(b.cx, my, 2, a); circle(b.cx, my, 1, withAlpha(shade(RED, 0.8), alpha)); }
  else if (kind === "flat") hline(b.cx - 2, my, 5, a);
}
// arms: angle in degrees from pointing down (0) to up (180), side -1 left / +1 right
function arm(b, side, angle, len = 7, alpha = 255) {
  const a = withAlpha(INK, alpha);
  const sx = b.cx + side * (b.rx - 1), sy = b.cy + 2;
  const rad = (angle * Math.PI) / 180;
  const dx = side * Math.sin(rad), dy = Math.cos(rad);
  for (let t = 0; t <= len; t++) { px(sx + dx * t, sy + dy * t, a); px(sx + dx * t + (side > 0 ? 1 : -1), sy + dy * t, a); }
  circle(sx + dx * len, sy + dy * len, 2, a);
  circle(sx + dx * len, sy + dy * len, 1, withAlpha(BASE.working, 0)); // keep hand solid ink
}
function laptop(o, base, keyGlow) {
  const x = o.x + 21, y = o.y + 55;
  rect(x, y, 22, 5, INK); rect(x + 1, y + 1, 20, 3, shade(base, 0.55)); // keyboard base
  rect(x + 2, y - 9, 18, 10, INK); rect(x + 3, y - 8, 16, 8, SCREEN); // screen (below the eyes)
  for (let i = 0; i < 2; i++) hline(x + 5, y - 6 + i * 3, 6 + ((i + keyGlow) % 3) * 3, shade(SCREEN, 0.6)); // code lines
}
function shadowBlob(o, alpha = 90) { ellipse(o.x + 32, o.y + 56, 15, 3, [0, 0, 0, alpha]); }
function exclaim(o, dy, alpha = 255) {
  const x = o.x + 31, y = o.y + 8 + dy;
  rect(x - 1, y - 1, 5, 12, withAlpha(INK, alpha)); rect(x, y, 3, 9, withAlpha(RED, alpha));
  rect(x - 1, y + 12, 5, 4, withAlpha(INK, alpha)); rect(x, y + 13, 3, 2, withAlpha(RED, alpha));
}
function zee(o, x, y, alpha) {
  const a = withAlpha(INK, alpha);
  hline(x, y, 5, a); px(x + 3, y + 1, a); px(x + 2, y + 2, a); px(x + 1, y + 3, a); hline(x, y + 4, 5, a);
}
function clock(o, x, y, frame) {
  circle(x, y, 7, INK); circle(x, y, 6, WHITE);
  px(x, y - 4, INK); px(x, y + 4, INK); px(x - 4, y, INK); px(x + 4, y, INK);
  const a = (frame / 8) * Math.PI * 2;
  for (let t = 1; t <= 4; t++) px(x + Math.round(Math.sin(a) * t), y - Math.round(Math.cos(a) * t), INK);
  px(x - 2, y, INK); px(x - 1, y, INK); // short hand
}
function miniClone(o, x, y, base, bob) {
  const cy = y - bob;
  ellipseOutline(x, cy, 6, 5, INK); ellipse(x, cy, 6, 5, base);
  for (let yy = 2; yy <= 5; yy++) for (let xx = -6; xx <= 6; xx++) if (inEllipse(xx, yy, 6, 5)) px(x + xx, cy + yy, shade(base, 0.78));
  circle(x - 2, cy - 1, 1, INK); circle(x + 2, cy - 1, 1, INK);
}
function dots(o, x, y, n) { for (let i = 0; i < n; i++) px(x + i * 3, y, INK); }

// ---------- animations ----------
const anims = {
  working(o, f) {
    const base = BASE.working; shadowBlob(o);
    const b = body(o, base, { dy: f % 2 });
    eyes(b, { look: 0 }); mouth(b, "smile");
    laptop(o, base, f);
    // typing: alternate arms down to the keyboard
    arm(b, -1, 35 + (f % 2) * 20, 7); arm(b, 1, 35 + ((f + 1) % 2) * 20, 7);
    if (f % 4 === 0) { px(o.x + 46, o.y + 30, YELLOW); px(o.x + 47, o.y + 28, YELLOW); }
    if (f % 4 === 2) { px(o.x + 18, o.y + 31, YELLOW); px(o.x + 17, o.y + 29, YELLOW); }
  },
  idle(o, f) {
    const base = BASE.idle; shadowBlob(o, 70);
    const sq = f < 3 ? 0 : 1;
    const b = body(o, base, { sq, dy: sq });
    eyes(b, { open: false }); mouth(b, "flat");
    arm(b, -1, 10, 5); arm(b, 1, 10, 5);
    // Z rising and fading
    const t = f / 6;
    zee(o, o.x + 44, o.y + 24 - Math.round(t * 10), 255 - Math.round(t * 150));
    if (f >= 3) zee(o, o.x + 48, o.y + 30 - Math.round((f - 3) * 2), 200);
  },
  needs_input(o, f) {
    const base = BASE.needs_input;
    const jump = [0, 3, 5, 5, 3, 0][f % 6];
    shadowBlob(o, 100 - jump * 10);
    const b = body(o, base, { dy: -jump, sq: jump ? -1 : 1 });
    eyes(b, { wide: true, look: 0 }); mouth(b, "o");
    arm(b, -1, 150 + jump * 4, 7); arm(b, 1, 150 + jump * 4, 7);
    exclaim(o, -jump + (f % 2));
  },
  delegating(o, f) {
    const base = BASE.delegating; shadowBlob(o);
    const b = body(o, base, { dy: f % 2 });
    eyes(b, { look: f < 4 ? -1 : 1 }); mouth(b, "smile");
    // point left for first half, right for second half
    arm(b, -1, f < 4 ? 90 : 20, 7); arm(b, 1, f < 4 ? 20 : 90, 7);
    const bob = [0, 1, 2, 1, 0, 1, 2, 1][f];
    miniClone(o, o.x + 9, o.y + 52, base, bob); miniClone(o, o.x + 55, o.y + 52, base, 2 - bob);
  },
  waiting_peer(o, f) {
    const base = BASE.waiting_peer; shadowBlob(o);
    const b = body(o, base, { dy: f % 4 === 0 ? 1 : 0 });
    eyes(b, { look: f < 4 ? -2 : 2 }); mouth(b, "flat");
    // crossed arms
    arm(b, -1, 70, 9); arm(b, 1, 70, 9);
    clock(o, o.x + 52, o.y + 14, f);
    dots(o, o.x + 12, o.y + 14, (f % 3) + 1);
  },
  gone(o, f) {
    const base = BASE.gone;
    const t = f / 7;
    const alpha = Math.round(255 * (1 - t));
    shadowBlob(o, Math.round(80 * (1 - t)));
    const b = body(o, base, { scale: 1 - t * 0.6, alpha, dy: Math.round(t * 6) });
    if (alpha > 40) { eyes(b, { open: f < 3, alpha }); mouth(b, "flat", alpha); }
    // rising particles
    for (let i = 0; i < 2; i++) {
      const pa = Math.round(220 * (1 - t));
      px(o.x + 22 + i * 20, o.y + 36 - f * 3 - i * 2, withAlpha(base, pa));
      px(o.x + 23 + i * 20, o.y + 35 - f * 3 - i * 2, withAlpha(WHITE, pa));
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
const manifest = { name: "default", frameSize: [F, F], scale: 2, animations: {} };
order.forEach((state, row) => {
  manifest.animations[state] = { sheet: "sheet.png", row, frames: frames[state], fps: fps[state], ...(state === "gone" ? { loop: false } : {}) };
});
writeFileSync("assets/packs/default/pack.json", JSON.stringify(manifest, null, 2) + "\n");
console.log(`default pack written (${png.width}x${png.height})`);
