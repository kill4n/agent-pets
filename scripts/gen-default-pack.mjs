import { PNG } from "pngjs";
import { writeFileSync, mkdirSync } from "node:fs";

const F = 64, COLS = 6, ROWS = 6;
const png = new PNG({ width: F * COLS, height: F * ROWS });
const px = (x, y, [r, g, b, a = 255]) => {
  if (x < 0 || y < 0 || x >= png.width || y >= png.height) return;
  const i = (png.width * y + x) << 2;
  png.data[i] = r; png.data[i + 1] = g; png.data[i + 2] = b; png.data[i + 3] = a;
};
const rect = (x, y, w, h, c) => { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) px(x + i, y + j, c); };
const circle = (cx, cy, r, c) => { for (let y = -r; y <= r; y++) for (let x = -r; x <= r; x++) if (x * x + y * y <= r * r) px(cx + x, cy + y, c); };

const BODY = { working: [90, 170, 255], idle: [150, 150, 170], needs_input: [255, 170, 60],
               delegating: [140, 220, 140], waiting_peer: [200, 150, 255], gone: [120, 120, 120] };
const BLACK = [20, 20, 30], WHITE = [250, 250, 250], RED = [230, 60, 60];
const order = ["working", "idle", "needs_input", "delegating", "waiting_peer", "gone"];

function body(ox, oy, color, squash = 0, alpha = 255) {
  const c = [...color, alpha];
  circle(ox + 32, oy + 40 + squash, 18 - squash, c);
  rect(ox + 18, oy + 44, 28, 10 - squash, c);
}
function eyes(ox, oy, open = true, dx = 0) {
  if (open) { circle(ox + 25 + dx, oy + 36, 3, WHITE); circle(ox + 39 + dx, oy + 36, 3, WHITE);
              circle(ox + 25 + dx, oy + 36, 1, BLACK); circle(ox + 39 + dx, oy + 36, 1, BLACK); }
  else { rect(ox + 22, oy + 36, 7, 2, BLACK); rect(ox + 36, oy + 36, 7, 2, BLACK); }
}

order.forEach((state, row) => {
  for (let col = 0; col < COLS; col++) {
    const ox = col * F, oy = row * F;
    switch (state) {
      case "working": { body(ox, oy, BODY[state], col % 2); eyes(ox, oy, true);
        rect(ox + 14 + (col % 3) * 2, oy + 50, 8, 4, BLACK); rect(ox + 42 - (col % 3) * 2, oy + 50, 8, 4, BLACK); break; }
      case "idle": { body(ox, oy, BODY[state], col < 2 ? 0 : 1); eyes(ox, oy, false);
        if (col % 2) rect(ox + 46, oy + 18 - col * 2, 4, 4, BLACK); break; }
      case "needs_input": { body(ox, oy, BODY[state]); eyes(ox, oy, true);
        if (col % 2 === 0) { rect(ox + 30, oy + 6, 4, 12, RED); rect(ox + 30, oy + 20, 4, 4, RED); } break; }
      case "delegating": { body(ox, oy, BODY[state]); eyes(ox, oy, true);
        const k = col % 3; circle(ox + 10, oy + 52 - k, 5, BODY[state]); circle(ox + 54, oy + 52 - k, 5, BODY[state]); break; }
      case "waiting_peer": { body(ox, oy, BODY[state]); eyes(ox, oy, true, col < 3 ? -3 : 3);
        circle(ox + 52, oy + 14, 7, WHITE); rect(ox + 52, oy + 9, 1, 5, BLACK);
        const a = (col / COLS) * Math.PI * 2; px(ox + 52 + Math.round(Math.sin(a) * 4), oy + 14 - Math.round(Math.cos(a) * 4), BLACK); break; }
      case "gone": { body(ox, oy, BODY[state], 0, Math.max(0, 255 - col * 45)); break; }
    }
  }
});

mkdirSync("assets/packs/default", { recursive: true });
writeFileSync("assets/packs/default/sheet.png", PNG.sync.write(png));
writeFileSync("assets/packs/default/pack.json", JSON.stringify({
  name: "default", frameSize: [F, F], scale: 2,
  animations: {
    working:      { sheet: "sheet.png", row: 0, frames: 6, fps: 10 },
    idle:         { sheet: "sheet.png", row: 1, frames: 4, fps: 4 },
    needs_input:  { sheet: "sheet.png", row: 2, frames: 4, fps: 6 },
    delegating:   { sheet: "sheet.png", row: 3, frames: 6, fps: 8 },
    waiting_peer: { sheet: "sheet.png", row: 4, frames: 6, fps: 4 },
    gone:         { sheet: "sheet.png", row: 5, frames: 6, fps: 12, loop: false },
  },
}, null, 2));
console.log("default pack written");
