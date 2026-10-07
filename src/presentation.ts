import type { State } from "./types";

export const smooth = (t: number) => {
  const x = Math.max(0, Math.min(1, t));
  return x * x * (3 - 2 * x);
};

// Mount the resting board in three readable beats: anticipation, hop, compression.
export function mountPose(menu: boolean, phase: string | null, time: number, reduced = false) {
  if (reduced) return { x: 0, y: 0, lean: 0, boardX: 0, compression: 0 };
  if (menu) return { x: -26, y: 0, lean: -.04, boardX: 14, compression: 0 };
  if (phase !== "intro") return { x: 0, y: 0, lean: 0, boardX: 0, compression: 0 };
  const anticipation = smooth(time / .2), hop = Math.max(0, Math.min(1, (time - .2) / .55));
  const landing = Math.max(0, time - .75);
  return {
    x: -26 + 26 * smooth(hop),
    y: time < .2 ? anticipation * 8 : -42 * Math.sin(Math.PI * hop),
    lean: -.08 + .20 * Math.sin(Math.PI * hop),
    boardX: 14 * (1 - smooth(hop)),
    compression: time >= .75 ? 11 * Math.exp(-landing * 12) : 0,
  };
}

// Presentation clocks never modify the saved simulation or its collision state.
export class RunPresentation {
  phase: "intro" | "crash" | null = null;
  time = 0;
  constructor(public reduced = false) {}
  get duration() { return this.reduced ? 0.18 : this.phase === "crash" ? 1.3 : 1.2; }
  get progress() { return this.phase ? Math.min(1, this.time / this.duration) : 1; }
  get simulationRate() { return this.phase === "intro" && !this.reduced ? 0.05 + 0.95 * smooth(this.time / 0.45) : 1; }
  begin(phase: "intro" | "crash") { this.phase = phase; this.time = 0; }
  clear() { this.phase = null; this.time = 0; }
  advance(dt: number) {
    if (!this.phase) return false;
    this.time = Math.min(this.duration, this.time + Math.max(0, dt));
    return this.time >= this.duration;
  }
  pose(s: State, ground: (x: number) => { y: number; gap: boolean }): State {
    if (this.phase !== "crash") return s;
    if (this.reduced) return { ...s, angle: Math.PI * 0.7 };
    const t = this.time;
    const x = s.x + Math.max(120, s.vx ?? s.speed) * 0.32 * (1 - Math.exp(-t / 0.32));
    const floor = ground(x);
    const falling = /chasm|gap/i.test(s.notice ?? "") || floor.gap;
    const y = falling ? s.y + Math.max(80, s.vy ?? 0) * t + 420 * t * t
      : Math.min(s.y - 100 * t + 440 * t * t, floor.y - 12 - Math.abs(Math.sin(t * 9)) * 18 * Math.exp(-t * 3));
    return { ...s, x, y, angle: s.angle + Math.PI * 1.25 * (1 - Math.exp(-t * 3)), grounded: false, wing_on: false, boost: 0, shield: false, immune: 0 };
  }
}

// A facade stays upright; its foundation follows every point of the hillside.
export function footing(x: number, width: number, ground: (x: number) => number) {
  return Array.from({ length: 13 }, (_, i) => {
    const wx = x - width / 2 + width * i / 12;
    return { x: wx, y: ground(wx) };
  });
}
