import type { Animation, Manifest } from "../shared/types";

export class Animator {
  private anim: Animation;
  private frame = 0;
  private acc = 0;
  private finished = false;

  constructor(private manifest: Manifest, name: string) {
    this.anim = manifest.animations[name] ?? manifest.animations.idle;
  }

  play(name: string) {
    const next = this.manifest.animations[name] ?? this.manifest.animations.idle;
    if (next === this.anim) return;
    this.anim = next; this.frame = 0; this.acc = 0; this.finished = false;
  }

  /** advance by dt ms; returns {sx, sy} source rect origin in the sheet */
  tick(dt: number): { sheet: string; sx: number; sy: number } {
    if (!this.finished) {
      this.acc += dt;
      const step = 1000 / Math.max(1, this.anim.fps);
      while (this.acc >= step) {
        this.acc -= step;
        if (this.frame + 1 >= this.anim.frames) {
          if (this.anim.loop) this.frame = 0; else { this.finished = true; break; }
        } else this.frame++;
      }
    }
    const [fw, fh] = this.manifest.frameSize;
    return { sheet: this.anim.sheet, sx: this.frame * fw, sy: this.anim.row * fh };
  }
}
