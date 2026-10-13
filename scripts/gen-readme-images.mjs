// Generates the README artwork from the sprite packs: a hero composition (docs/images/hero.png)
// and animated state strips (docs/images/slime-states.gif, docs/images/fox-states.gif).
// Everything is drawn from the real sheets, so the pictures always match the packs.
import { PNG } from "pngjs";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import gifenc from "gifenc";

const { GIFEncoder, quantize, applyPalette } = gifenc;
const OUT = "docs/images";
mkdirSync(OUT, { recursive: true });

// ---------- packs ----------
function loadPack(dir) {
  const manifest = JSON.parse(readFileSync(`${dir}/pack.json`, "utf8"));
  const sheets = {};
  for (const a of Object.values(manifest.animations)) sheets[a.sheet] ??= PNG.sync.read(readFileSync(`${dir}/${a.sheet}`));
  return { manifest, sheets };
}
const slime = loadPack("assets/packs/default");
const fox = loadPack("assets/packs/fox");

// ---------- canvas helpers ----------
function canvas(w, h) { return new PNG({ width: w, height: h }); }
function px(img, x, y, [r, g, b, a = 255]) {
  if (x < 0 || y < 0 || x >= img.width || y >= img.height || a <= 0) return;
  const i = (img.width * y + x) << 2;
  const da = img.data[i + 3] / 255, sa = a / 255, oa = sa + da * (1 - sa);
  for (let k = 0; k < 3; k++) img.data[i + k] = Math.round(([r, g, b][k] * sa + img.data[i + k] * da * (1 - sa)) / (oa || 1));
  img.data[i + 3] = Math.round(oa * 255);
}
function rect(img, x, y, w, h, c) { for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) px(img, x + i, y + j, c); }
function roundRect(img, x, y, w, h, r, c) {
  for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) {
    const dx = Math.max(r - i, 0, i - (w - 1 - r)), dy = Math.max(r - j, 0, j - (h - 1 - r));
    if (dx * dx + dy * dy <= r * r) px(img, x + i, y + j, c);
  }
}
// draw one frame of an animation, upscaled by `scale` (nearest neighbour), alpha-blended
function blit(img, pack, state, frame, x, y, scale = 2, alpha = 1) {
  const a = pack.manifest.animations[state];
  const [fw, fh] = pack.manifest.frameSize;
  const sheet = pack.sheets[a.sheet];
  const f = frame % a.frames;
  for (let yy = 0; yy < fh; yy++) for (let xx = 0; xx < fw; xx++) {
    const i = (sheet.width * (a.row * fh + yy) + (f * fw + xx)) << 2;
    const al = sheet.data[i + 3] * alpha;
    if (al === 0) continue;
    const c = [sheet.data[i], sheet.data[i + 1], sheet.data[i + 2], al];
    for (let sy = 0; sy < scale; sy++) for (let sx = 0; sx < scale; sx++) px(img, x + xx * scale + sx, y + yy * scale + sy, c);
  }
}

// ---------- tiny bitmap font (5x7) for labels ----------
const FONT = {
  A: ["01110","10001","10001","11111","10001","10001","10001"], B: ["11110","10001","10001","11110","10001","10001","11110"],
  C: ["01110","10001","10000","10000","10000","10001","01110"], D: ["11110","10001","10001","10001","10001","10001","11110"],
  E: ["11111","10000","10000","11110","10000","10000","11111"], F: ["11111","10000","10000","11110","10000","10000","10000"],
  G: ["01110","10001","10000","10111","10001","10001","01111"], H: ["10001","10001","10001","11111","10001","10001","10001"],
  I: ["01110","00100","00100","00100","00100","00100","01110"], K: ["10001","10010","10100","11000","10100","10010","10001"],
  L: ["10000","10000","10000","10000","10000","10000","11111"], M: ["10001","11011","10101","10101","10001","10001","10001"],
  N: ["10001","11001","10101","10011","10001","10001","10001"], O: ["01110","10001","10001","10001","10001","10001","01110"],
  P: ["11110","10001","10001","11110","10000","10000","10000"], R: ["11110","10001","10001","11110","10100","10010","10001"],
  S: ["01111","10000","10000","01110","00001","00001","11110"], T: ["11111","00100","00100","00100","00100","00100","00100"],
  U: ["10001","10001","10001","10001","10001","10001","01110"], V: ["10001","10001","10001","10001","10001","01010","00100"],
  W: ["10001","10001","10001","10101","10101","10101","01010"], X: ["10001","10001","01010","00100","01010","10001","10001"],
  Y: ["10001","10001","01010","00100","00100","00100","00100"], Z: ["11111","00001","00010","00100","01000","10000","11111"],
  "0": ["01110","10001","10011","10101","11001","10001","01110"], "1": ["00100","01100","00100","00100","00100","00100","01110"],
  "2": ["01110","10001","00001","00010","00100","01000","11111"], "3": ["11110","00001","00001","01110","00001","00001","11110"],
  "4": ["00010","00110","01010","10010","11111","00010","00010"], "5": ["11111","10000","11110","00001","00001","10001","01110"],
  "6": ["00110","01000","10000","11110","10001","10001","01110"], "7": ["11111","00001","00010","00100","01000","01000","01000"],
  "8": ["01110","10001","10001","01110","10001","10001","01110"], "9": ["01110","10001","10001","01111","00001","00010","01100"],
  "-": ["00000","00000","00000","11111","00000","00000","00000"], " ": ["00000","00000","00000","00000","00000","00000","00000"],
  "!": ["00100","00100","00100","00100","00100","00000","00100"], "?": ["01110","10001","00001","00010","00100","00000","00100"],
  ".": ["00000","00000","00000","00000","00000","00000","00100"],
  ":": ["00000","00100","00000","00000","00000","00100","00000"],
};
function textWidth(s, scale) { return s.length * 6 * scale - scale; }
function text(img, s, x, y, c, scale = 1) {
  let cx = x;
  for (const ch of s.toUpperCase()) {
    const g = FONT[ch] ?? FONT["?"];
    g.forEach((row, j) => [...row].forEach((bit, i) => { if (bit === "1") rect(img, cx + i * scale, y + j * scale, scale, scale, c); }));
    cx += 6 * scale;
  }
}
// the in-app name pill under a sprite
function namePill(img, label, cx, y, maxW, scale = 1) {
  const w = Math.min(maxW, textWidth(label, scale) + 12 * scale), h = 9 * scale + 4;
  roundRect(img, cx - Math.floor(w / 2), y, w, h, Math.floor(h / 2), [20, 20, 30, 190]);
  text(img, label, cx - Math.floor(textWidth(label, scale) / 2), y + 2 + scale, [255, 255, 255], scale);
}
// a comic speech bubble above a sprite with 1-3 lines of text
function bubble(img, lines, cx, bottomY, scale = 1) {
  const lw = Math.max(...lines.map(l => textWidth(l, scale)));
  const w = lw + 14 * scale, h = lines.length * 9 * scale + 8 * scale;
  const x = cx - Math.floor(w / 2), y = bottomY - h - 8 * scale;
  roundRect(img, x - 2, y - 2, w + 4, h + 4, 8, [34, 34, 34]);
  roundRect(img, x, y, w, h, 7, [255, 251, 232]);
  // tail
  for (let t = 0; t < 7 * scale; t++) { const hw = Math.max(0, 6 * scale - t); rect(img, cx - hw, y + h + t, hw * 2 + 1, 1, t < 2 ? [34, 34, 34] : [255, 251, 232]); }
  rect(img, cx - 7 * scale, y + h, 1, 2, [34, 34, 34]); rect(img, cx + 7 * scale, y + h, 1, 2, [34, 34, 34]);
  lines.forEach((l, i) => text(img, l, x + 7 * scale, y + 4 * scale + i * 9 * scale, [26, 26, 34], scale));
}
function desktopBackground(img) {
  // dark panel with a soft diagonal gradient and a faint grid, readable on light and dark GitHub themes
  for (let y = 0; y < img.height; y++) for (let x = 0; x < img.width; x++) {
    const t = (x / img.width + y / img.height) / 2;
    const c = [Math.round(30 + 18 * t), Math.round(32 + 14 * t), Math.round(48 + 26 * t), 255];
    px(img, x, y, c);
  }
  for (let y = 0; y < img.height; y += 24) rect(img, 0, y, img.width, 1, [255, 255, 255, 10]);
  for (let x = 0; x < img.width; x += 24) rect(img, x, 0, 1, img.height, [255, 255, 255, 10]);
}

// ---------- hero.png ----------
{
  const W = 1200, H = 480, S = 2;
  const img = canvas(W, H);
  desktopBackground(img);
  const floor = 400;
  const put = (pack, state, frame, cx, name, scale = S, extra) => {
    const [fw, fh] = pack.manifest.frameSize;
    const x = cx - (fw * scale) / 2, y = floor - fh * scale;
    blit(img, pack, state, frame, x, y, scale);
    namePill(img, name, cx, floor + 6, fw * scale, scale === 1 ? 1 : 2);
    if (extra) extra(cx, y);
  };
  put(slime, "working", 2, 150, "api-server", S);
  put(slime, "needs_input", 2, 400, "buddy-34", S, (cx, y) => bubble(img, ["BUDDY-34", "NEEDS YOUR INPUT", "BASH: RM -RF BUILD"], cx, y - 4, 2));
  put(slime, "delegating", 3, 630, "pipeline", S);
  put(slime, "working", 5, 745, "explore", 1);   // subagent: half size, hugging its parent
  put(fox, "waiting_peer", 1, 910, "docs-site", 3);
  put(slime, "idle", 1, 1090, "buddy-dev", S);
  text(img, "ONE SPRITE PER CLAUDE CODE SESSION", 24, 20, [235, 235, 245], 3);
  text(img, "WORKING . NEEDS INPUT . DELEGATING . WAITING FOR A PEER . IDLE", 24, 52, [160, 168, 200], 2);
  writeFileSync(`${OUT}/hero.png`, PNG.sync.write(img));
  console.log("hero.png", W, "x", H);
}

// ---------- animated state strips ----------
function stateStrip(pack, file, scale, label) {
  const states = ["working", "idle", "needs_input", "delegating", "waiting_peer", "gone"];
  const names = ["WORKING", "IDLE", "NEEDS INPUT", "DELEGATING", "WAITING PEER", "DONE"];
  const [fw, fh] = pack.manifest.frameSize;
  const cell = fw * scale + 24, W = cell * states.length + 24, H = fh * scale + 70;
  const gif = GIFEncoder();
  const FPS = 12, SECONDS = 4;
  for (let n = 0; n < FPS * SECONDS; n++) {
    const img = canvas(W, H);
    desktopBackground(img);
    states.forEach((s, i) => {
      const a = pack.manifest.animations[s];
      const t = n / FPS;
      const frame = a.loop === false ? Math.min(a.frames - 1, Math.floor(t * a.fps)) : Math.floor(t * a.fps) % a.frames;
      const x = 24 + i * cell, y = 16;
      blit(img, pack, s, frame, x, y, scale);
      const lw = textWidth(names[i], 1);
      text(img, names[i], x + Math.floor((fw * scale - lw) / 2), y + fh * scale + 12, [220, 224, 240], 1);
    });
    text(img, label, 24, H - 20, [140, 148, 180], 1);
    const palette = quantize(img.data, 256);
    const index = applyPalette(img.data, palette);
    gif.writeFrame(index, W, H, { palette, delay: Math.round(1000 / FPS), repeat: 0 });
  }
  gif.finish();
  writeFileSync(`${OUT}/${file}`, gif.bytes());
  console.log(file, W, "x", H);
}
stateStrip(slime, "slime-states.gif", 2, "DEFAULT PACK - SLIME");
stateStrip(fox, "fox-states.gif", 3, "FOX PACK");
