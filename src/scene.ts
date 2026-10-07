import type { State, Mode, Feature } from "./types";
import { FollowCamera, PLAY_ZOOM, terrainFraming } from "./camera";
import { RunPresentation, smooth, footing, mountPose } from "./presentation";
const TAU = Math.PI * 2;
type Particle = {
  x: number;
  y: number;
  vx: number;
  vy: number;
  life: number;
  max: number;
  size: number;
  color: string;
  kind?: "flag" | "snow";
  rotation?: number;
};
type Palette = {
  sky: string[];
  mist: string;
  rim: string;
  stone: string;
  layers: string[];
  dust: string;
  sun: string;
};
const palettes: Palette[] = [
  {
    sky: ["#101e32", "#426177", "#c79580", "#f7cfa3"],
    mist: "#eec4a0",
    rim: "#ffca9a",
    stone: "#293847",
    layers: ["#6c7481", "#566878", "#3b5367", "#293e52"],
    dust: "#f8b987",
    sun: "#ffdfb0",
  },
  {
    sky: ["#091e30", "#22536b", "#6aabaf", "#c9ddd0"],
    mist: "#c0e5d7",
    rim: "#a8f4df",
    stone: "#183c46",
    layers: ["#69969e", "#41777f", "#2c5866", "#1a3b4b"],
    dust: "#9ce0c7",
    sun: "#ddf6d9",
  },
  {
    sky: ["#15172f", "#4d4669", "#a9788e", "#eed1b6"],
    mist: "#eec1cc",
    rim: "#efbadd",
    stone: "#302e48",
    layers: ["#938199", "#6c647d", "#494c69", "#313b57"],
    dust: "#f1bad8",
    sun: "#fbe2c3",
  },
];
function hash(n: number, seed = 0) {
  let a = (Math.imul(n | 0, 374761393) ^ (seed | 0)) >>> 0;
  a = Math.imul(a ^ (a >>> 13), 1274126177) >>> 0;
  return ((a ^ (a >>> 16)) >>> 0) / 4294967295;
}
function noise(x: number, seed: number) {
  const i = Math.floor(x),
    t = x - i,
    u = t * t * (3 - 2 * t);
  return hash(i, seed) * (1 - u) + hash(i + 1, seed) * u;
}
const mix = (a: number, b: number, t: number) => a + (b - a) * t;
export class Scene {
  private ctx: CanvasRenderingContext2D;
  private skin: "classic" | "tower" = "classic";
  setSkin(skin: "classic" | "tower") { this.skin = skin; }
  private width = 1600;
  private height = 900;
  private ratio = 1;
  private zoom = PLAY_ZOOM;
  private riderFrameX = 0.2;
  private riderFrameY = 0.46;
  private camera = new FollowCamera();
  private camX = 0;
  private camY = 0;
  private particles: Particle[] = [];
  private smallSprites = new Map<string, HTMLCanvasElement>();
  private lastGrounded = true;
  private lastCoins = 0;
  private impact = 0;
  private prevSeed = -1;
  private elapsed = 0;
  private titleAnchor = 0;
  private titleElevation = 900;
  private launchAge: number | null = null;
  private launchFrame = { x: .70, y: .80, zoom: .52 };
  private riddenFlags = new Map<number, { from: number; to: number }>();
  private lostFlags = new Set<string>();
  private ropeBreaks = new Map<number, number>();
  private lastPruneX = -Infinity;
  private snowCarry = 0;
  private mountLanded = false;
  readonly presentation = new RunPresentation(matchMedia("(prefers-reduced-motion: reduce)").matches);
  beginEntrance(fromTitle = true) {
    this.presentation.reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
    this.presentation.begin("intro");
    this.mountLanded = false;
    this.launchAge = fromTitle && !this.presentation.reduced ? 0 : null;
    this.launchFrame = { x: this.riderFrameX, y: this.riderFrameY, zoom: this.zoom };
  }
  beginCrash(s: State) {
    this.presentation.reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
    this.presentation.begin("crash");
    if (!this.presentation.reduced) this.emit(s.x, s.y + 18, 28, "#e7e1cd", 2.2);
  }
  constructor(
    private canvas: HTMLCanvasElement,
    private art: Record<string, HTMLImageElement>,
  ) {
    this.ctx = canvas.getContext("2d", { alpha: false })!;
    new ResizeObserver(() => this.resize()).observe(canvas);
    this.resize();
  }
  private resize() {
    const r = this.canvas.getBoundingClientRect();
    const dpr = Math.min(devicePixelRatio || 1, 2);
    this.canvas.width = Math.round(r.width * dpr);
    this.canvas.height = Math.round(r.height * dpr);
    this.ratio = r.height / 900;
    this.width = r.width / this.ratio;
    this.ctx.setTransform(dpr * this.ratio, 0, 0, dpr * this.ratio, 0, 0);
  }
  private surface(s: State, x: number) {
    const segments = s.terrain_segments;
    if (segments?.length) {
      let lo=0,hi=segments.length-1;
      while(lo<hi) { const mid=(lo+hi)>>>1; if(segments[mid].x1<x)lo=mid+1;else hi=mid; }
      const segment = segments[lo];
      const l = segment.x1 - segment.x0, t = (x - segment.x0) / l;
      const a = 2 * segment.y0 - 2 * segment.y1 + l * (segment.m0 + segment.m1);
      const b = -3 * segment.y0 + 3 * segment.y1 - l * (2 * segment.m0 + segment.m1);
      const y = t < 0 ? segment.y0 + (x - segment.x0) * segment.m0 : t > 1 ? segment.y1 + (x - segment.x1) * segment.m1 : a * t ** 3 + b * t ** 2 + l * segment.m0 * t + segment.y0;
      const slope = t < 0 ? segment.m0 : t > 1 ? segment.m1 : (3 * a * t ** 2 + 2 * b * t + l * segment.m0) / l;
      return { y, slope, gap: (s.gaps ?? []).some(([a, b]) => x > a && x < b) };
    }
    const a = s.terrain;
    if (a.length === 0) return { y: 0, gap: false, slope: 0.2 };
    const index = Math.max(
      0,
      Math.min(a.length - 2, Math.floor((x - a[0].x) / (a[1].x - a[0].x))),
    );
    const p = a[index],
      q = a[index + 1];
    const t = (x - p.x) / (q.x - p.x);
    return {
      y: mix(p.y, q.y, t),
      gap: s.gaps ? s.gaps.some(([a, b]) => x > a && x < b) : t < 0.5 ? p.gap : q.gap,
      slope: p.slope ?? (q.y - p.y) / (q.x - p.x),
    };
  }
  private sprite(
    name: string,
    x: number,
    y: number,
    size: number,
    alpha = 1,
    rotation = 0,
  ) {
    const img = this.art[name];
    if (!img) return;
    const c = this.ctx;
    c.save();
    c.globalAlpha *= alpha;
    c.translate(x, y);
    c.rotate(rotation);
    // Horde copies occupy only a few screen pixels. Reuse a small raster
    // instead of filtering the full extracted texture thousands of times.
    let source: CanvasImageSource = img;
    if(size < 24) {
      let cached=this.smallSprites.get(name);
      if(!cached) {
        cached=document.createElement("canvas");cached.width=64;cached.height=64;
        cached.getContext("2d")!.drawImage(img,0,0,64,64);this.smallSprites.set(name,cached);
      }
      source=cached;
    }
    c.drawImage(source, -size / 2, -size / 2, size, size);
    c.restore();
  }
  private glow(x: number, y: number, r: number, color: string, alpha = 0.35) {
    const c = this.ctx;
    c.save();
    c.globalAlpha = alpha;
    const g = c.createRadialGradient(x, y, 0, x, y, r);
    g.addColorStop(0, color);
    g.addColorStop(1, color + "00");
    c.fillStyle = g;
    c.fillRect(x - r, y - r, r * 2, r * 2);
    c.restore();
  }
  private emit(x: number, y: number, count: number, color: string, force = 1) {
    for (let i = 0; i < count; i++) {
      const t = hash((i + this.particles.length + this.elapsed * 100) | 0);
      const life = 0.3 + t * 0.65;
      this.particles.push({
        x,
        y,
        vx: (t - 0.8) * 140 * force,
        vy: -30 - hash(i * 3 + 17) * 70 * force,
        life,
        max: life,
        size: 1 + t * 3,
        color,
      });
    }
    if (this.particles.length > 180)
      this.particles.splice(0, this.particles.length - 180);
  }
  private snow(s: State, count: number, burst = false) {
    const slope = this.surface(s, s.x).slope, a = Math.atan(slope);
    for (let i = 0; i < count; i++) {
      const t = hash((this.elapsed * 1000 + i * 37 + this.particles.length) | 0);
      const life = .3 + t * .45, back = (burst ? 180 : 100) + t * 160;
      this.particles.push({ kind: "snow", x: s.x - 19, y: this.surface(s,s.x-19).y - 3,
        vx: -Math.cos(a)*back + (s.vx ?? s.speed)*.12,
        vy: -Math.sin(a)*back - 65 - t*(burst ? 220 : 110), life, max: life,
        size: (burst ? 3 : 2) + t*5, color: "#f4f1df" });
    }
    if(this.particles.length>220)this.particles.splice(0,this.particles.length-220);
  }
  render(s: State, mode: Mode, dt: number) {
    const moving = mode === "play" || mode === "crash" || mode === "menu";
    if (!moving) dt = 0;
    this.elapsed += dt;
    if (mode === "menu" || mode === "crash") this.launchAge = null;
    else if (mode === "play" && this.launchAge != null) this.launchAge += dt;
    s = this.presentation.pose(s, x => this.surface(s, x));
    const c = this.ctx,
      w = this.width,
      h = this.height;
    const seed = s.seed ?? 0x53ca;
    const segment = s.time / 90;
    const phase = segment - Math.floor(segment),
      blend = phase * phase * (3 - 2 * phase);
    const index = (Math.floor(segment) + (seed % 3)) % 3;
    const current = palettes[index],
      next = palettes[(index + 1) % 3];
    const color = (a: string, b: string) => {
      const hex = (v: string, shift: number) =>
        (parseInt(v.slice(1), 16) >> shift) & 255;
      return (
        "#" +
        [16, 8, 0]
          .map((shift) =>
            Math.round(mix(hex(a, shift), hex(b, shift), blend))
              .toString(16)
              .padStart(2, "0"),
          )
          .join("")
      );
    };
    const p: Palette = {
      sky: current.sky.map((v, i) => color(v, next.sky[i])),
      layers: current.layers.map((v, i) => color(v, next.layers[i])),
      mist: color(current.mist, next.mist),
      rim: color(current.rim, next.rim),
      stone: color(current.stone, next.stone),
      dust: color(current.dust, next.dust),
      sun: color(current.sun, next.sun),
    };
    const menu = mode === "menu";
    const shortLandscape = w > 900 && h*this.ratio < 540;
    const menuFrameX = shortLandscape ? .42 : .70;
    if (seed !== this.prevSeed) {
      this.prevSeed = seed;
      this.snowCarry = 0; this.lastPruneX = -Infinity;
      const initialFrame = terrainFraming(this.surface(s, s.x).slope, w, h);
      this.zoom = menu ? 0.52 : initialFrame.zoom;
      this.riderFrameX = menu ? menuFrameX : initialFrame.playerX / w;
      this.riderFrameY = menu ? (w < 900 ? .46 : .80) : initialFrame.baseline / h;
      if (!menu) this.launchAge = null;
      this.camX = s.x;
      this.camY = s.y;
      this.camera.reset(s.x, s.y);
      this.lastGrounded = s.grounded;
      this.particles = [];
      this.riddenFlags.clear(); this.lostFlags.clear(); this.ropeBreaks.clear();
    }
    if(Math.abs(s.x-this.lastPruneX)>400) {
      this.lastPruneX=s.x;
      const liveRopes = new Set(s.features.filter(f => f.kind === "rail").map(f => f.id));
      for (const id of this.riddenFlags.keys()) if (!liveRopes.has(id)) this.riddenFlags.delete(id);
      for (const id of this.ropeBreaks.keys()) if (!liveRopes.has(id)) this.ropeBreaks.delete(id);
      for (const key of this.lostFlags) if (!liveRopes.has(parseInt(key,10))) this.lostFlags.delete(key);
    }
    const framing = terrainFraming(this.surface(s, s.x).slope, w, h);
    if (!menu) for (const wall of s.features.filter(f => f.active && f.kind === "wall")) {
      const left = wall.x - wall.width / 2, right = wall.x + wall.width / 2;
      const approach = Math.max(0, 1 - Math.max(0, left - s.x) / 1400);
      const departure = Math.max(0, 1 - Math.max(0, s.x - right) / 700);
      const t = Math.min(approach, departure), influence = t * t * (3 - 2 * t);
      const wallHeight = wall.y - (wall.y2 ?? wall.y - 1100);
      const wallZoom = Math.max(PLAY_ZOOM * 0.72, Math.min(PLAY_ZOOM, h * 0.55 / (wallHeight * 1.25)));
      framing.zoom = Math.min(framing.zoom, mix(PLAY_ZOOM, wallZoom, influence));
      framing.baseline = Math.max(framing.baseline, h * (0.46 + 0.20 * influence));
      framing.playerX = Math.max(framing.playerX, w * (0.2 + 0.04 * influence));
    }
    // Frame the terrain immediately behind the rider, rather than shrinking the
    // entire scene to fit a distant summit on a long descent.
    if (!menu) {
      const trail = Math.min(1000, framing.playerX / framing.zoom * 0.85);
      const floorY = this.surface(s, s.x).y;
      let rise = 0;
      for (let i = 1; i <= 12; i++) {
        const point = this.surface(s, s.x - trail * i / 12);
        if (!point.gap) rise = Math.max(rise, floorY - point.y);
      }
      framing.baseline = Math.max(framing.baseline, Math.min(h * 0.74, rise * framing.zoom + h * 0.14));
    }
    const response = 1 - Math.exp(-dt * 3);
    this.riderFrameX = mix(this.riderFrameX, menu ? menuFrameX : framing.playerX / w, response);
    this.riderFrameY = mix(this.riderFrameY, menu ? (w < 900 ? .46 : .80) : framing.baseline / h, response);
    const targetZoom = menu ? 0.52 : framing.zoom;
    this.zoom = mix(this.zoom, targetZoom, 1 - Math.exp(-dt * 2));
    // Recording 48–50.5s: leave the village at the right/bottom of frame,
    // then settle over ~2.5s. Jump cancels the pose, not this camera travel.
    const launch = this.launchAge == null ? 1 : Math.min(1, this.launchAge / 2.5);
    if (this.launchAge != null) {
      this.riderFrameX = mix(this.launchFrame.x, framing.playerX / w, 1-(1-launch)**2);
      this.riderFrameY = mix(this.launchFrame.y, framing.baseline / h, smooth(launch));
      this.zoom = mix(this.launchFrame.zoom, targetZoom, smooth(launch));
      if (launch >= 1) this.launchAge = null;
    }
    const z = this.zoom;
    const playerX = w * this.riderFrameX,
      baseline = h * this.riderFrameY;
    if (mode === "play" || (mode === "crash" && !this.presentation.reduced))
      this.camera.update(s.x, s.y, (s.vx ?? s.speed) * this.presentation.simulationRate * smooth(launch), s.grounded || s.rail != null, dt);
    else if (menu) this.camera.reset(s.x, s.y);
    // Bound forward drift on narrow screens, without snapping at takeoff/landing.
    this.camX = Math.max(s.x - w * 0.18 / z, Math.min(s.x + w * 0.05 / z, this.camera.x));
    this.camY = this.camera.y;
    const floor = this.surface(s, s.x).y;
    const sx = (x: number) => (x - this.camX) * z + playerX,
      sy = (y: number) => (y - this.camY) * z + baseline;
    if (menu) {
      this.titleAnchor = (w*(shortLandscape?.25:.50) - playerX) / z;
      this.titleElevation = (baseline + 18*z - h*(w<900?.22:.28)) / z;
    }
    const sky = c.createLinearGradient(0, 0, 0, h);
    p.sky.forEach((color, i) => sky.addColorStop(i / 3, color));
    c.fillStyle = sky;
    c.fillRect(0, 0, w, h);
    // Sparse celestial details and luminous, layered cloud banks.
    for (let i = 0; i < 70; i++) {
      const x = hash(i, seed) * w,
        y = hash(i + 300, seed) * h * 0.42;
      const alpha = (1 - y / (h * 0.45)) * 0.55;
      c.fillStyle = `rgba(238,235,233,${alpha})`;
      c.beginPath();
      c.arc(x, y, i % 13 === 0 ? 1.5 : 0.7, 0, Math.PI * 2);
      c.fill();
    }
    const sunX = w * 0.7,
      sunY = h * 0.3;
    this.glow(sunX, sunY, this.skin === "classic" ? 110 : 220, p.sun, 0.12);
    c.fillStyle = p.sun;
    c.beginPath();
    c.arc(sunX, sunY, this.skin === "classic" ? 22 : 43, 0, Math.PI * 2);
    c.fill();
    for (let i = 0; i < 7; i++) {
      const x =
        ((hash(i + 90, seed) * w + this.elapsed * (1 + i * 0.12)) % (w + 550)) -
        100;
      const y = 105 + hash(i + 40, seed) * 210;
      c.save();
      c.translate(x, y);
      c.scale(3.8, 0.22);
      this.glow(0, 0, 65, p.mist, 0.055);
      c.restore();
    }
    // Every point is fixed in world space. Scenery translates; its shape never resamples.
    for (let layer = 0; layer < 4; layer++) {
      const spacing = 350 - layer * 45, offset = s.x * (0.018 + layer * 0.028);
      const base = h * (0.43 + layer * 0.08), first = Math.floor((offset - spacing) / spacing);
      const mountainShade = c.createLinearGradient(0, base - 180, 0, h);
      mountainShade.addColorStop(0, p.layers[layer]); mountainShade.addColorStop(1, p.layers[Math.min(3,layer+1)]);
      c.fillStyle = mountainShade;
      c.beginPath(); c.moveTo(-spacing, h);
      for (let i = first; (i - first) * spacing < w + spacing * 3; i++) {
        const valleyX = i * spacing - offset;
        const valleyY = base - hash(i * 3 + layer * 101, seed) * 55;
        const peakX = valleyX + spacing * (0.38 + hash(i * 7 + layer * 13, seed) * 0.2);
        const peakY = base - 65 - hash(i * 11 + layer * 71, seed) * (100 - layer * 10);
        c.lineTo(valleyX, valleyY);
        c.quadraticCurveTo(mix(valleyX, peakX, .5), mix(valleyY,peakY,.35), peakX-8, peakY+4);
        c.quadraticCurveTo(peakX,peakY,peakX+12,peakY+7);
        c.quadraticCurveTo(peakX+spacing*.08,peakY+20,peakX+spacing*.16,peakY+25+hash(i+layer*91,seed)*25);
      }
      c.lineTo(w + spacing, h); c.closePath(); c.fill();
    }
    // Sparse fixed pines add depth without making the mountains undulate.
    const treeOffset = s.x * 0.19, treeSpacing = 95;
    c.fillStyle = p.layers[3]; c.globalAlpha = 0.45;
    for (let i = Math.floor(treeOffset / treeSpacing) - 2; i * treeSpacing - treeOffset < w + 100; i++) {
      const x = i * treeSpacing - treeOffset + hash(i + 214, seed) * 45;
      const y = h * 0.70 + hash(i + 104, seed) * 65;
      const height = 35 + hash(i + 51, seed) * 115;
      c.fillRect(x-height*.014,y-height*.7,height*.028,height*.7);
      for (let branch = 0; branch < 4; branch++) {
        const crown = y-height+branch*height*.16, spread=height*(.07+branch*.035);
        c.beginPath(); c.moveTo(x,crown); c.quadraticCurveTo(x-spread*.45,crown+height*.21,x-spread,crown+height*.3);
        c.lineTo(x+spread,crown+height*.3); c.quadraticCurveTo(x+spread*.45,crown+height*.21,x,crown); c.fill();
      }
    }
    c.globalAlpha = 1;
    // The supplied tower is an unreachable horizon landmark, with bounded approach.
    const approach = 1 - Math.exp(-s.x / 28000),
      tx = w * 0.88 - approach * 22,
      ty = h * 0.365;
    const portrait = this.art.Player;
    if (portrait && this.skin === "tower") {
      const size = (w < 650 ? 205 : 245) + approach * 65,
        iw = (size * portrait.width) / portrait.height;
      this.glow(tx, ty + 35, size * 0.8, "#5fc3e4", 0.04);
      c.save();
      c.globalAlpha = 0.3;
      c.drawImage(portrait, tx - iw / 2, ty - size / 2, iw, size);
      c.globalAlpha = 0.12;
      c.strokeStyle = "#95c6d8";
      for (let i = 0; i < 3; i++) {
        c.beginPath();
        c.ellipse(
          tx,
          ty + size * 0.47,
          65 + i * 33,
          8 + i * 5,
          0,
          0,
          Math.PI * 2,
        );
        c.stroke();
      }
      c.restore();
    }
    // The title belongs to the launch village, so it leaves with the landscape.
    if (s.x < 4000) {
      const tx = sx(this.titleAnchor), ty = sy(this.surface(s, 0).y - this.titleElevation);
      const size = Math.min(shortLandscape ? 135 : 170, w * .155) * z / .52;
      c.save(); c.textAlign = "center"; c.fillStyle = "#f4edda";
      c.font = `900 ${size}px Outfit, sans-serif`; c.fillText("ALTO’S", tx, ty);
      c.font = `800 ${size * .72}px Outfit, sans-serif`; c.fillText("TOWER", tx, ty + size * .85);
      c.restore();
    }
    // Cliff faces sit behind the continuous foreground, with their feet buried in it.
    for (const f of s.features) {
      if (f.active && f.kind === "wall" && sx(f.x + f.width) > -200 && sx(f.x - f.width) < w + 200)
        this.feature(f, s, sx, sy, z, tx, ty, p);
    }
    // Build individual solid islands, leaving actual gaps visible through the horde.
    const left = this.camX - playerX / z - 80,
      right = this.camX + (w - playerX) / z + 80;
    const spans: { start: number; end: number }[] = [];
    let start = left;
    for (const [a, b] of s.gaps ?? []) {
      if (b <= start || a >= right) continue;
      if (a > start) spans.push({ start, end: Math.min(a, right) });
      start = Math.max(start, b);
    }
    if (start < right) spans.push({ start, end: right });
    for (const span of spans) {
      const outline = new Path2D();
      outline.moveTo(sx(span.start), h + 200);
      for (let x = span.start; x <= span.end; x += 8)
        outline.lineTo(sx(x), sy(this.surface(s, x).y));
      outline.lineTo(sx(span.end), sy(this.surface(s, span.end).y));
      outline.lineTo(sx(span.end), h + 200);
      outline.closePath();
      const rock = c.createLinearGradient(0, h * 0.55, 0, h);
      rock.addColorStop(0, this.skin === "classic" ? "#eee9db" : p.stone);
      rock.addColorStop(1, this.skin === "classic" ? "#dadccf" : "#0c1c2a");
      c.fillStyle = rock;
      c.fill(outline);
      c.save();
      c.clip(outline);
      // Irregularly packed, rotated Basic enemies: avalanche strata rather than a tile grid.
      const flow = (s.terrain_time ?? s.time) * (s.horde_flow ?? 20);
      const first = Math.floor((span.start - flow) / 28);
      // Wider cliff views must not multiply sprite work by four. Keep retained
      // enemies at the same world positions while sampling the distant crowd.
      const stride = Math.max(1, Math.ceil(PLAY_ZOOM / (z * 1.8)));
      for (let row = 0; row < (this.skin === "tower" ? 11 : 0); row++) {
        for (let i = Math.ceil(first / stride) * stride; i * 28 + flow < span.end + 30; i += stride) {
          const r = hash(i * 37 + row * 571, seed),
            wx = i * 28 + flow + (r - 0.5) * 17 + (row % 2) * 14 + Math.sin(s.time * 0.35 + i + row) * 4;
          const depth = row * 23 + hash(i * 11 + row * 13, seed) * 17;
          const surface = this.surface(s, wx);
          const y = surface.y + depth + Math.sin(s.time * 0.4 + i * 0.7 + row) * 4;
          if(sy(y)<-30 || sy(y)>h+30)continue;
          this.sprite(
            "Basic",
            sx(wx),
            sy(y),
            z * (15 + r * 17),
            (0.13 * Math.exp(-depth / 120) + 0.018) * (row === 0 ? 1.7 : 1),
            Math.atan(surface.slope) + (r - 0.5) * 0.7,
          );
        }
      }
      c.restore();
      c.save();
      c.strokeStyle = this.skin === "classic" ? "#fff8e8" : p.rim;
      c.lineWidth = 2;
      c.shadowColor = p.rim;
      c.shadowBlur = this.skin === "classic" ? 0 : 9;
      c.beginPath();
      for (let x = span.start; x <= span.end; x += 8) {
        const y = sy(this.surface(s, x).y);
        if (x === span.start) c.moveTo(sx(x), y);
        else c.lineTo(sx(x), y);
      }
      c.stroke();
      c.restore();
      if (span.end < right - 20) {
        const x = sx(span.end),
          y = sy(this.surface(s, span.end - 8).y);
        this.glow(x, y, 80, p.rim, 0.08);
      }
    }
    const layer = (f: Feature) => ["pine", "hut", "ruin", "ice", "wall"].includes(f.kind) ? 0 : f.kind === "rail" ? 1 : f.kind === "rock" ? 2 : 3;
    if (s.x < 3500) {
      for (const [wx, width] of [[-650, 330], [280, 250]]) this.feature({ id: -20 + wx, kind: "hut", x: wx, y: this.surface(s, wx).y, width, active: true }, s, sx, sy, z, tx, ty, p);
      const wx = -1350, gy = this.surface(s, wx).y;
      c.save(); c.translate(sx(wx), sy(gy)); c.scale(z, z);
      c.fillStyle = "#dce2d6"; c.beginPath(); c.moveTo(-115, this.surface(s, wx - 115).y - gy + 8); c.lineTo(-70, -650); c.lineTo(70, -650); c.lineTo(115, this.surface(s, wx + 115).y - gy + 8); c.closePath(); c.fill();
      c.fillStyle = "#9aaea5"; c.fillRect(36, -620, 28, 610);
      c.fillStyle = "#a96057"; c.beginPath(); c.moveTo(-90, -650); c.lineTo(0, -750); c.lineTo(90, -650); c.closePath(); c.fill();
      c.strokeStyle = "#43585d"; c.lineWidth = 10; c.beginPath(); c.moveTo(-15, 0); c.lineTo(-15, -85); c.quadraticCurveTo(0, -115, 15, -85); c.lineTo(15, 0); c.stroke();
      c.translate(0, -655); c.rotate(this.elapsed * .32);
      for (let blade = 0; blade < 4; blade++) { c.rotate(Math.PI / 2); c.fillStyle = "#e5e6d8"; c.strokeStyle = "#5b7778"; c.lineWidth = 6; c.beginPath(); c.moveTo(-22, -45); c.lineTo(-52, -350); c.lineTo(50, -350); c.lineTo(22, -95); c.closePath(); c.fill(); c.stroke(); c.beginPath(); c.moveTo(0, -50); c.lineTo(0, -350); c.stroke(); }
      c.fillStyle = "#b67760"; c.beginPath(); c.arc(0, 0, 24, 0, TAU); c.fill(); c.restore();
      c.strokeStyle = "#596b64"; c.lineWidth = 6 * z;
      for (let wx = -400; wx < -40; wx += 70) { const fy = this.surface(s, wx).y; c.beginPath(); c.moveTo(sx(wx), sy(fy)); c.lineTo(sx(wx), sy(fy - 55)); c.moveTo(sx(wx), sy(fy - 34)); c.lineTo(sx(wx + 70), sy(this.surface(s, wx + 70).y - 34)); c.stroke(); }
    }
    for (const f of [...s.features].sort((a, b) => layer(a) - layer(b))) {
      if ((f.active || s.rope_states?.[f.id]?.broken_at != null) && f.kind !== "wall" && sx(f.x+f.width/2) > -400 && sx(f.x-f.width/2) < w + 400)
        this.feature(f, s, sx, sy, z, tx, ty, p);
    }
    if (s.camp && !s.chase && sx(s.camp.x) > -150 && sx(s.camp.x) < w + 200) {
      this.feature({ id: -1, kind: "hut", x: s.camp.x, y: s.camp.y + 18, width: 130, active: true }, s, sx, sy, z, tx, ty, p);
      const x = sx(s.camp.x - 95), y = sy(s.camp.y + 18);
      this.glow(x, y - 12 * z, 60 * z, "#ffd48e", 0.28);
      c.fillStyle = "#ffe0a3"; c.beginPath();
      c.moveTo(x - 8 * z, y); c.quadraticCurveTo(x + 3 * z, y - 34 * z, x + 10 * z, y); c.fill();
    }
    if (s.chase) {
      const ch = s.chase, actualX = sx(ch.x), actualY = sy(ch.y);
      const x = actualX, y = actualY;
      // Keep the pursuer at its actual world position, with a persistent gap badge.
      c.save(); c.fillStyle = "#18323ce8"; c.fillRect(24, 145, 132, 42);
      c.fillStyle = s.x-ch.x < 400 ? "#ffad8b" : "#ead6b3"; c.font = "600 22px Outfit, sans-serif";
      c.fillText(`‹ ${Math.max(0,Math.round((s.x-ch.x)/40))} m`, 38, 175); c.restore();
      this.glow(x, y - 15, 40, "#f2a079", 0.2);
      c.save(); c.translate(x, y); c.rotate(Math.atan(this.surface(s, ch.x).slope)); c.scale(z * 1.6, z * 1.6);
      if (this.skin === "tower") {
        this.sprite("Tank", 0, -4, 37);
        this.sprite("Ranged", 0, -29, 24);
      } else {
        c.strokeStyle = "#a97869"; c.lineWidth = 5; c.lineCap = "round";
        c.beginPath(); c.ellipse(0, 0, 17, 8, 0, 0, Math.PI * 2); c.stroke();
        c.beginPath(); c.moveTo(12, 1); c.lineTo(19, -20); c.lineTo(28, -21);
        const stride = Math.sin(s.time * 17) * 5;
        for (const leg of [-12, -4, 5, 12]) { c.moveTo(leg, 5); c.lineTo(leg + stride * (leg < 0 ? 1 : -1), 17); }
        c.stroke(); c.fillStyle = "#d7b392"; c.beginPath(); c.arc(0, -29, 5, 0, Math.PI * 2); c.fill();
        c.fillStyle = "#8e615f"; c.beginPath(); c.moveTo(-5, -22); c.lineTo(-11, 0); c.lineTo(9, -3); c.closePath(); c.fill();
      }
      c.strokeStyle = "#f2a079"; c.lineWidth = 3; c.beginPath(); c.moveTo(-3, -24);
      c.quadraticCurveTo(-35, -15, -55, -27 + Math.sin(s.time * 8) * 3); c.stroke(); c.restore();
    }
    // Persistent dust, scarf and energy trail tie the rider to movement and contact.
    if (mode === "play") {
      if (s.grounded && !this.lastGrounded) {
        this.snow(s, 28, true);
        this.impact = 0.35;
      }
      if (s.grounded && s.speed > 250 && dt > 0) {
        this.snowCarry += dt * (32 + Math.min(50, s.speed / 25));
        const count = Math.floor(this.snowCarry); this.snowCarry -= count;
        this.snow(s, count);
      } else this.snowCarry = 0;
      if(this.presentation.phase === "intro" && this.presentation.time >= .75 && !this.mountLanded) {
        this.mountLanded = true; this.snow(s, 22, true); this.impact = .3;
      }
      if (s.proximity) {
        const ground=this.surface(s,s.x).y;
        if (Math.random()<0.8) this.emit(s.x-25,ground-3,2,p.dust,0.7);
      }
      if (s.wall != null && Math.random()<0.6) this.emit(s.x-12,s.y+18,2,p.dust,0.7);
      if (s.rail != null && Math.random() < 0.7)
        this.emit(s.x - 14, s.y + 18, 2, "#a3ffd6", 1.3);
      if (s.coins > this.lastCoins) this.emit(s.x, s.y, 7, "#ffe5a9", 1.4);
    }
    this.lastGrounded = s.grounded;
    this.lastCoins = s.coins;
    this.impact = Math.max(0, this.impact - dt);
    c.save();
    for (const part of this.particles) {
      if (mode === "play" || mode === "crash") {
        part.life -= dt;
        part.x += part.vx * dt;
        part.y += part.vy * dt;
        part.vy += (part.kind === "flag" ? 150 : part.kind === "snow" ? 320 : 80) * dt;
      }
      c.globalAlpha = Math.max(0, part.life / part.max);
      c.fillStyle = part.color;
      c.beginPath();
      if (part.kind === "flag") { c.save(); c.translate(sx(part.x), sy(part.y)); c.rotate((part.rotation ?? 0) + Math.sin((part.max - part.life) * 9) * .5); c.moveTo(-9*z, 0); c.lineTo(9*z, 0); c.lineTo(0, 22*z); c.closePath(); c.fill(); c.restore(); }
      else { c.arc(sx(part.x), sy(part.y), part.size * z, 0, Math.PI * 2); c.fill(); }
    }
    c.restore();
    this.particles = this.particles.filter((q) => q.life > 0);
    const crashing = this.presentation.phase === "crash";
    const mount = mountPose(menu, this.presentation.phase, this.presentation.time, this.presentation.reduced);
    const entrance = this.presentation.phase === "intro" && !this.presentation.reduced ? 1 - smooth(this.presentation.progress) : 0;
    const terrainLean = s.grounded ? -Math.atan(this.surface(s,s.x).slope)*.22 : 0;
    const bodyLean = mount.lean + (menu || entrance ? 0 : terrainLean);
    const crouch = crashing ? 5 : (menu ? 0 : s.grounded ? this.impact * 18 + mount.compression + Math.min(3,s.speed/800) : s.wing_on ? 1 : 5);
    const rx = sx(s.x),
      ry = sy(s.y);
    if ((s.boost ?? 0) > 0) {
      c.save(); c.strokeStyle = "#fff3bd"; c.lineWidth = 2.5;
      c.globalAlpha = Math.min(0.9, (s.boost ?? 0) / 0.12);
      const heading = Math.atan2(s.vy ?? 0, s.vx ?? s.speed);
      const ux = Math.cos(heading), uy = Math.sin(heading);
      this.glow(rx, ry - 10 * z, 48 * z, "#fff1a6", 0.38);
      for (let i = 0; i < 3; i++) {
        const length = (120 + i * 65) * z;
        const offset = (i - 1) * 7 * z;
        c.beginPath(); c.moveTo(rx - ux * 22 * z - uy * offset, ry - uy * 22 * z + ux * offset);
        c.lineTo(rx - ux * length - uy * offset, ry - uy * length + ux * offset); c.stroke();
      }
      c.beginPath(); c.ellipse(rx, ry - 8 * z, 39 * z, 29 * z, heading, 0, Math.PI * 2); c.stroke();
      c.restore();
    }
    const altitude = Math.max(0, floor - s.y - 18);
    if (!this.surface(s, s.x).gap) {
      c.save();
      c.globalAlpha = 0.18 * Math.max(0, 1 - altitude / 280);
      c.fillStyle = "#050c13";
      c.beginPath();
      c.ellipse(rx, sy(floor) + 3, 30 * z, 5 * z, 0, 0, Math.PI * 2);
      c.fill();
      c.restore();
    }
    c.save();
    const neck = this.skin === "classic" && s.rider === 5 ? { x: 16, y: -13 } : s.wing_on ? { x: 13, y: -8 } : { x: 0, y: -9 };
    const riderScale = 1.6 * (PLAY_ZOOM / z) ** 0.75;
    const nx = (mount.x + neck.x*Math.cos(bodyLean)-neck.y*Math.sin(bodyLean)) * riderScale;
    const ny = 17 + (mount.y + crouch + neck.x*Math.sin(bodyLean)+neck.y*Math.cos(bodyLean) - 17) * riderScale;
    const scarfX = rx + (nx * Math.cos(s.angle) - ny * Math.sin(s.angle)) * z;
    const scarfY = ry + (nx * Math.sin(s.angle) + ny * Math.cos(s.angle)) * z;
    const scarfLength = (24 + Math.min(1, (s.scarf_length ?? s.wing) / 6) * 320) * z;
    c.strokeStyle = s.wing_ready || (s.boost ?? 0) > 0 ? "#fff0b9" : s.riders[s.rider].color;
    c.globalAlpha = 0.95;
    c.lineWidth = 3.5 * z;
    c.beginPath();
    c.moveTo(scarfX, scarfY);
    c.bezierCurveTo(
      scarfX - scarfLength * 0.25,
      scarfY + 4 * z,
      scarfX - scarfLength * 0.6,
      scarfY - 10 * z + Math.sin(s.time * 8) * 6 * z,
      scarfX - scarfLength,
      scarfY - 5 * z,
    );
    c.stroke();
    c.restore();
    c.save();
    c.translate(rx, ry);
    c.rotate(s.angle);
    c.scale(z, z);
    // Preserve the board contact point while keeping the small reference rider
    // readable after widening the world view.
    c.translate(0, 17); c.scale(riderScale, riderScale); c.translate(0, -17);
    if (s.wing_on && !crashing) {
      const billow = this.presentation.reduced ? 0 : Math.sin(s.time * 7) * 2;
      const cloth = c.createLinearGradient(-50, -28, 12, 16); cloth.addColorStop(0, "#bed0ce"); cloth.addColorStop(.5, "#6e929b"); cloth.addColorStop(1, "#36586b");
      c.fillStyle = cloth; c.strokeStyle = "#d9e4d9"; c.lineWidth = 1.2;
      c.beginPath(); c.moveTo(12,-10); c.quadraticCurveTo(-5,-24,-17,-32); c.bezierCurveTo(-34,-33+billow,-51,-23,-59,-16); c.quadraticCurveTo(-40,-4,-32,13); c.lineTo(-12,3); c.closePath(); c.fill(); c.stroke();
      c.beginPath(); c.moveTo(8,-5); c.quadraticCurveTo(-6,10,-15,23); c.quadraticCurveTo(-31,32+billow,-49,27); c.quadraticCurveTo(-39,17,-32,13); c.lineTo(-12,0); c.closePath(); c.fill(); c.stroke();
      c.strokeStyle = "#e4e7d36b";
      for (const [wx,wy] of [[-17,-32],[-45,-25],[-15,23],[-49,27]]) { c.beginPath(); c.moveTo(5,-7); c.quadraticCurveTo(-18,0,wx,wy); c.stroke(); }
    }
    c.save(); c.translate(mount.x, crouch + mount.y);
    c.rotate(bodyLean);
    if (this.skin === "tower") this.glow(0, 0, 46, s.riders[s.rider].color, 0.14);
    c.shadowColor = s.riders[s.rider].color;
    c.shadowBlur = 12;
    if (this.skin === "tower") this.sprite(s.riders[s.rider].name, 0, -2, 31);
    else if (s.rider === 5) {
      // Felipe's distinct quadruped silhouette makes his reference profile visible.
      c.shadowBlur = 0; c.fillStyle = "#263f49"; c.strokeStyle = "#263f49";
      c.lineWidth = 5; c.lineCap = "round";
      c.beginPath(); c.ellipse(-3, -1, 17, 7, 0, 0, Math.PI * 2);
      c.strokeStyle = "#e4e7dc"; c.lineWidth = 2; c.stroke(); c.fill();
      c.beginPath(); c.moveTo(10, 0); c.lineTo(16, -21); c.lineTo(25, -22);
      c.moveTo(16, -20); c.lineTo(16, -29); c.moveTo(21, -22); c.lineTo(22, -29);
      c.moveTo(-17, -2); c.lineTo(-25, -10);
      for (const leg of [-13, -5, 6, 12]) { c.moveTo(leg, 3); c.lineTo(leg - 3 - Math.sin(this.presentation.time * 8 + leg) * 8 * entrance, 17 - crouch); }
      c.strokeStyle = "#e4e7dc"; c.lineWidth = 7; c.stroke();
      c.strokeStyle = "#263f49"; c.lineWidth = 5; c.stroke();
      c.fillStyle = "#c7755d"; c.fillRect(11, -13, 8, 3);
    } else if (s.wing_on) {
      c.shadowBlur = 0; c.fillStyle = "#263f49"; c.strokeStyle = "#e4e7dc"; c.lineWidth = 2;
      c.beginPath(); c.arc(18,-13,5,0,Math.PI*2); c.fill(); c.stroke();
      c.strokeStyle = "#263f49"; c.lineWidth = 5; c.lineCap = "round";
      c.beginPath(); c.moveTo(12,-8); c.lineTo(-12,0); c.lineTo(-32,13); c.moveTo(8,-8); c.lineTo(-17,-32); c.moveTo(5,-6); c.lineTo(-15,23); c.stroke();
      c.fillStyle = "#c7755d"; c.fillRect(14,-20,9,3);
    } else {
      c.shadowBlur = 0; c.fillStyle = "#263f49";
      c.beginPath(); c.arc(1, -15, 5, 0, Math.PI * 2);
      c.strokeStyle = "#e4e7dc"; c.lineWidth = 2; c.stroke(); c.fill();
      c.strokeStyle = "#263f49"; c.lineWidth = 5; c.lineCap = "round";
      c.beginPath(); c.moveTo(0, -8); c.lineTo(menu ? 0 : -4, 2);
      c.lineTo(menu ? 3 : 6, menu ? 9 : 7); c.lineTo(menu ? 5 : 9, 17 - crouch);
      c.moveTo(menu ? 0 : -4, 2); c.lineTo(menu ? -4 : -10, 9);
      c.lineTo(menu ? -6 : -9 - Math.sin(this.presentation.time * 7) * 20 * entrance, 17 - crouch);
      const arm = menu ? 7 + Math.sin(this.elapsed*1.8)*.8 : s.grounded ? -3-this.impact*16 : -12;
      c.moveTo(-1, -5); c.lineTo(8, arm+3); c.lineTo(13, arm);
      c.strokeStyle = "#e4e7dc"; c.lineWidth = 7; c.stroke();
      c.strokeStyle = "#263f49"; c.lineWidth = 5; c.stroke();
      c.fillStyle = "#c7755d"; c.fillRect(-4, -20, 9, 4);
    }
    c.shadowBlur = 0;
    c.strokeStyle = s.wing_ready || (s.boost ?? 0) > 0 ? "#fff0b9" : s.riders[s.rider].color;
    c.lineWidth = 3;
    c.beginPath(); c.ellipse(neck.x, neck.y, 5, 1.8, 0, 0, Math.PI * 2); c.stroke();
    c.restore();
    c.save();
    c.translate(mount.boardX, 0);
    if (s.wing_on) c.globalAlpha = 0;
    if (crashing) { c.translate(-35 * this.presentation.progress, 25 * this.presentation.progress); c.rotate(-this.presentation.time * 3); }
    c.strokeStyle = "#e4e2d5";
    c.lineWidth = 3;
    c.lineCap = "round";
    c.beginPath();
    c.moveTo(-25, 17);
    c.quadraticCurveTo(-8, 24, 13, 21);
    c.quadraticCurveTo(25, 21, 30, 15);
    c.stroke();
    c.strokeStyle = "#65ded5";
    c.lineWidth = 1;
    c.beginPath();
    c.moveTo(-24, 23);
    c.lineTo(23, 24);
    c.stroke();
    c.restore();
    if (s.immune > 0 || s.shield) {
      c.strokeStyle = "#a0eff1";
      c.lineWidth = 1.5;
      c.beginPath();
      c.ellipse(3 + mount.x, -7 + mount.y, 42, 45, 0, 0, Math.PI * 2);
      c.stroke();
    }
    c.restore();
    if (this.impact > 0) {
      c.save();
      c.globalAlpha = this.impact;
      c.strokeStyle = this.skin === "classic" ? "#fff8e8" : p.rim;
      c.lineWidth = 2;
      c.beginPath();
      c.ellipse(
        rx,
        sy(floor),
        60 * (1 - this.impact),
        12 * (1 - this.impact),
        0,
        0,
        Math.PI * 2,
      );
      c.stroke();
      c.restore();
    }
    const fog = c.createLinearGradient(0, h * 0.78, 0, h);
    fog.addColorStop(0, "#08172400");
    fog.addColorStop(1, "#081724a0");
    c.fillStyle = fog;
    c.fillRect(0, h * 0.78, w, h * 0.22);
    for (let i = 0; i < 16; i++) {
      const x =
          (hash(i + 821, seed) * w -
            this.elapsed * (10 + hash(i) * 12) +
            w * 100) %
          w,
        y = hash(i + 923, seed) * h;
      c.fillStyle = "#f1d4b329";
      c.beginPath();
      c.arc(x, y, hash(i + 2) * 1.8 + 0.4, 0, Math.PI * 2);
      c.fill();
    }
    if (s.slow) {
      c.fillStyle = "#a9cef50c";
      c.fillRect(0, 0, w, h);
      c.strokeStyle = "#addcea2b";
      c.lineWidth = 1;
      for (let i = 0; i < 5; i++) {
        c.beginPath();
        c.ellipse(rx, ry, 180 + i * 75, 90 + i * 45, 0.15, 0, Math.PI * 2);
        c.stroke();
      }
    }
  }
  private feature(
    f: Feature,
    s: State,
    sx: (x: number) => number,
    sy: (y: number) => number,
    z: number,
    tx: number,
    ty: number,
    p: Palette,
  ) {
    const c = this.ctx,
      x = sx(f.x),
      y = sy(f.y),
      width = f.width * z,
      phase =
        s.effects?.find((e) => e.id === f.id)?.phase ??
        (((s.time * 0.65 + f.id * 0.173) % 1) + 1) % 1;
    c.save();
    if (f.kind === "pine") {
      c.fillStyle = this.skin === "classic" ? "#21383e" : "#2c4a50";
      const height = width * 2.7;
      for (let tier = 0; tier < 4; tier++) {
        const yy = y - height + height * tier * 0.2;
        const half = width * (0.22 + tier * 0.10);
        c.beginPath(); c.moveTo(x, yy); c.lineTo(x - half, yy + height * 0.4);
        c.lineTo(x + half, yy + height * 0.4); c.closePath(); c.fill();
      }
      c.fillRect(x - 3 * z, y - height * 0.25, 6 * z, height * 0.25);
    } else if (f.kind === "hut") {
      const appearance = f.appearance;
      const height = (appearance?.facade_height ?? f.width * .65) * z;
      const roofRise = appearance ? width * .5 * Math.tan(appearance.roof_pitch) : height * .3;
      const doorOffset = appearance?.door_offset ?? .24;
      const feet = footing(f.x, f.width, wx => this.surface(s, wx).y);
      const facade = new Path2D(); facade.moveTo(x - width / 2, y - height);
      facade.lineTo(x + width / 2, y - height);
      for (const point of [...feet].reverse()) facade.lineTo(sx(point.x), sy(point.y) + 8 * z);
      facade.closePath();
      const plaster = c.createLinearGradient(x - width / 2, y - height, x + width / 2, y + height * .4);
      plaster.addColorStop(0, this.skin === "classic" ? "#ecebdc" : "#45616b");
      plaster.addColorStop(1, this.skin === "classic" ? "#b7c4bd" : "#263e4d");
      c.fillStyle = plaster; c.fill(facade);
      c.save(); c.clip(facade);
      c.fillStyle = "#142c3320"; c.fillRect(x + width * .35, y - height, width * .15, height * 3);
      c.strokeStyle = "#82958a"; c.lineWidth = 8 * z; c.beginPath();
      feet.forEach((point, i) => { if (!i) c.moveTo(sx(point.x), sy(point.y) + 2 * z); else c.lineTo(sx(point.x), sy(point.y) + 2 * z); });
      c.stroke();
      const doorFloor = sy(this.surface(s, f.x + f.width * doorOffset).y) + 5 * z;
      c.fillStyle = "#33464c"; c.fillRect(x + width * (doorOffset-.08), y - height * .5, width * .16, doorFloor - y + height * .5);
      c.fillStyle = "#ebce98";
      const windowCount = Math.max(1,Math.min(6,Math.floor(f.width / (appearance?.window_spacing ?? f.width*.4))));
      for (let i=0;i<windowCount;i++) {
        const windowX = -.4+(i+.5)*.8/windowCount;
        const paneWidth=Math.min(width*.12,width*.55/windowCount);
        c.fillRect(x + width * windowX - paneWidth*.5, y - height * .68, paneWidth, height * .18);
        c.strokeStyle = "#657b78"; c.lineWidth = 2 * z;
        c.strokeRect(x + width * windowX-paneWidth*.5, y - height * .68, paneWidth, height * .18);
        c.beginPath(); c.moveTo(x + width * windowX, y - height * .68); c.lineTo(x + width * windowX, y - height * .5); c.stroke();
      }
      c.restore();
      c.fillStyle = "#54666b"; c.fillRect(x + width * .23, y - height-roofRise*.7, width * .09, roofRise*.6);
      c.fillStyle = this.skin === "classic" ? ["#ab6358","#a47e65","#768889","#b88c75"][(appearance?.palette??0)%4] : ["#779aa0","#65868d","#92a594","#748497"][(appearance?.palette??0)%4];
      c.beginPath(); c.moveTo(x - width * 0.57, y - height); c.lineTo(x, y - height-roofRise);
      c.lineTo(x + width * 0.57, y - height); c.closePath(); c.fill();
      c.strokeStyle = "#f2dfbf"; c.lineWidth = 3 * z;
      c.beginPath(); c.moveTo(x - width * .57, y - height); c.lineTo(x, y - height-roofRise); c.lineTo(x + width * .57, y - height); c.stroke();
      c.fillStyle = "#152c3429"; c.fillRect(x - width / 2, y - height, width, 6 * z);
    } else if (f.kind === "ruin") {
      const height = width * 1.2;
      c.fillStyle = "#647574";
      for (const offset of [-.39, .39]) {
        const center = f.x + f.width * offset, top = y - height * (offset < 0 ? 1 : .9);
        const feet = footing(center, f.width * .22, wx => this.surface(s, wx).y);
        c.beginPath(); c.moveTo(sx(center - f.width * .11), top); c.lineTo(sx(center + f.width * .11), top);
        for (const point of [...feet].reverse()) c.lineTo(sx(point.x), sy(point.y) + 8 * z);
        c.closePath(); c.fill();
      }
      c.fillRect(x - width / 2, y - height, width, height * 0.2);
      c.strokeStyle = "#b3b7a4"; c.lineWidth = 2 * z;
      c.beginPath(); c.moveTo(x - width * 0.35, y - height); c.lineTo(x - width * 0.35, y); c.stroke();
    } else if (f.kind === "ice") {
      c.strokeStyle = "#9ed8e0"; c.lineWidth = 6 * z; c.lineCap = "round";
      c.beginPath();
      for (let i = 0; i <= 24; i++) {
        const wx = f.x - f.width / 2 + f.width * i / 24;
        const yy = sy(this.surface(s, wx).y) - 2 * z;
        if (!i) c.moveTo(sx(wx), yy); else c.lineTo(sx(wx), yy);
      }
      c.stroke();
    } else if (["magnet", "feather", "shield"].includes(f.kind)) {
      this.glow(x, y, 40 * z, "#c5eed8", 0.18);
      c.strokeStyle = "#d7fae6"; c.lineWidth = 2 * z; c.fillStyle = "#20464f";
      c.beginPath(); c.arc(x, y, 18 * z, 0, Math.PI * 2); c.fill(); c.stroke();
      if (this.skin === "tower") this.sprite(({magnet:"Ranged",feather:"Protector",shield:"Tank"} as Record<string,string>)[f.kind], x, y, 23*z);
      else if (f.kind === "magnet") {
        c.strokeStyle = "#9ecbdc"; c.lineWidth = 5*z;
        c.beginPath(); c.moveTo(x-7*z,y-7*z); c.lineTo(x-7*z,y+2*z);
        c.arc(x,y+2*z,7*z,Math.PI,0,true); c.lineTo(x+7*z,y-7*z); c.stroke();
      } else if (f.kind === "feather") {
        c.fillStyle = "#fff3d5"; c.beginPath(); c.ellipse(x,y,4*z,11*z,0.5,0,Math.PI*2); c.fill();
        c.beginPath(); c.moveTo(x-5*z,y+12*z); c.lineTo(x+5*z,y-12*z); c.stroke();
      } else {
        c.beginPath(); c.moveTo(x-8*z,y-7*z); c.lineTo(x+8*z,y-7*z); c.lineTo(x+6*z,y+4*z);
        c.lineTo(x,y+10*z); c.lineTo(x-6*z,y+4*z); c.closePath(); c.stroke();
      }
    } else if (f.kind === "rock") {
      const outline = s.rock_outline ?? [[-1,22],[-0.8,-9],[-0.1,-22],[0.7,-16],[1,22]];
      c.fillStyle = "#344249"; c.strokeStyle = "#aab3af"; c.lineWidth = 1.5 * z;
      c.beginPath();
      outline.forEach(([ox, oy], i) => { if (!i) c.moveTo(x + ox * width / 2, y + oy * z); else c.lineTo(x + ox * width / 2, y + oy * z); });
      c.closePath(); c.fill(); c.stroke();
      if (this.skin === "tower") this.sprite("Tank",x,y,34*z);
    } else if (f.kind === "coin") {
      const big = f.variant === 1;
      const pulse = 0.9 + Math.sin(this.elapsed * 4 + f.id) * 0.08;
      this.glow(x, y, big ? 32 : 18, "#ffd798", big ? 0.4 : 0.2);
      c.translate(x, y);
      if (big) {
        const radius = 11 * z;
        const gold = c.createRadialGradient(-radius * .3, -radius * .4, 0, 0, 0, radius);
        gold.addColorStop(0, "#fff5c7"); gold.addColorStop(.65, "#eec567"); gold.addColorStop(1, "#bb853c");
        c.fillStyle = gold; c.strokeStyle = "#fff0bb"; c.lineWidth = 1.5 * z;
        c.beginPath(); c.arc(0, 0, radius, 0, Math.PI * 2); c.fill(); c.stroke();
        c.strokeStyle = "#99713b"; c.lineWidth = z;
        c.beginPath(); c.arc(0, 0, radius * .76, 0, Math.PI * 2); c.stroke();
        c.fillStyle = "#684923"; c.font = `bold ${10 * z}px sans-serif`; c.textAlign = "center"; c.textBaseline = "middle";
        c.fillText("10", 0, .5 * z); c.restore(); return;
      }
      if (this.skin === "classic") {
        c.fillStyle = "#f4cc76";
        c.beginPath(); c.arc(0, 0, 5 * z, 0, Math.PI * 2); c.fill();
        c.strokeStyle = "#fff0bc"; c.lineWidth = z;
        c.beginPath(); c.arc(0, 0, 3 * z, 0, Math.PI * 2); c.stroke();
        c.restore(); return;
      }
      c.rotate(Math.PI / 4);
      c.fillStyle = "#fff0c0";
      c.fillRect(-3 * pulse * z, -3 * pulse * z, 6 * pulse * z, 6 * pulse * z);
      c.strokeStyle = "#ffd395";
      c.lineWidth = 1;
      c.strokeRect(-5 * z, -5 * z, 10 * z, 10 * z);
    } else if (f.kind === "rail") {
      const y2 = sy(f.y2 ?? f.y);
      const left = x - width / 2,
        right = x + width / 2;
      if (f.variant === 1) {
        c.fillStyle = this.skin === "classic" ? "#d9dfd4" : "#304b57";
        c.beginPath(); c.moveTo(left, y); c.lineTo(right, y2);
        for (const point of footing(f.x, f.width, wx => this.surface(s, wx).y).reverse()) c.lineTo(sx(point.x), sy(point.y) + 8 * z);
        c.closePath(); c.fill();
        c.strokeStyle = this.skin === "classic" ? "#b77360" : "#9dcec6";
        c.lineWidth = 8 * z; c.beginPath(); c.moveTo(left, y); c.lineTo(right, y2); c.stroke();
        c.fillStyle = "#4d696c";
        const windows = Math.min(18, Math.floor(f.width / 95));
        for (let i = 0; i < windows; i++) {
          const t = (i + 0.5) / windows, yy = mix(y, y2, t);
          c.fillRect(mix(left, right, t) - 5 * z, yy + 18 * z, 10 * z, 18 * z);
        }
        c.restore(); return;
      }
      c.strokeStyle = this.skin === "classic" ? "#455356" : "#a3ffd6";
      c.lineWidth = 4 * z;
      c.shadowColor = "#83ffd0";
      c.shadowBlur = this.skin === "classic" ? 0 : 11;
      const rope = s.rope_states?.[f.id];
      let passed = this.riddenFlags.get(f.id);
      if (rope) {
        if (!passed) { passed = { from: rope.ridden_to, to: rope.ridden_to }; this.riddenFlags.set(f.id, passed); }
        else passed.to = Math.max(passed.to, rope.ridden_to);
      }
      if (rope?.broken_at != null && !this.ropeBreaks.has(f.id)) this.ropeBreaks.set(f.id, this.elapsed - Math.max(0, s.time - rope.broken_at));
      const age = this.ropeBreaks.has(f.id) ? Math.max(0, this.elapsed - this.ropeBreaks.get(f.id)!) : 0;
      const split = Math.max(.1, Math.min(.9, ((rope?.ridden_to ?? f.x) - (f.x - f.width / 2)) / f.width));
      const drop = (80 * age + 450 * age * age) * z;
      const cableY = (t: number) => mix(y, y2, t) + 4 * (f.sag ?? 0) * t * (1-t) * z + drop * (t < split ? (t/split)**2 : ((1-t)/(1-split))**2);
      c.beginPath();
      for (let i = 0; i <= 64; i++) {
        const t = i / 64;
        if (i === 0 || (age > 0 && (i-1)/64 < split && t >= split)) c.moveTo(mix(left,right,t), cableY(t));
        else c.lineTo(mix(left,right,t), cableY(t));
      }
      if (this.skin === "classic") {
        c.strokeStyle = "#e3ddbf"; c.lineWidth = 7 * z; c.stroke();
        c.strokeStyle = "#455356"; c.lineWidth = 3 * z;
      }
      c.stroke();
      if (rope && rope.load > .6 && age === 0) {
        const contact = Math.max(0, Math.min(1, (rope.ridden_to - (f.x-f.width/2))/f.width));
        c.strokeStyle = "#efb082"; c.lineWidth = 2*z;
        for (let strand=0;strand<3;strand++) { const xx=mix(left,right,contact)+(strand-1)*8*z, yy=cableY(contact); c.beginPath(); c.moveTo(xx-3*z,yy); c.lineTo(xx+3*z,yy+12*z*(rope.load-.5)); c.stroke(); }
      }
      const flagColors = ["#d18772", "#89b2a4", "#d5c79d"];
      for (let i = 0; i < f.width / 100; i++) {
        const t = (i + 0.5) / (f.width / 100);
        const wx = f.x - f.width / 2 + t * f.width;
        const key = `${f.id}:${i}`;
        if (passed && wx > passed.from && wx <= passed.to && !this.lostFlags.has(key)) {
          this.lostFlags.add(key);
          this.particles.push({ kind: "flag", rotation: hash(i,f.id)*2-1, x: wx, y: mix(f.y,f.y2??f.y,t)+4*(f.sag??0)*t*(1-t), vx: hash(i+20,f.id)*60-30, vy: 24, life: 3, max: 3, size: 9, color: flagColors[i%3] });
        }
        if (this.lostFlags.has(key)) continue;
        const yy = cableY(t);
        const xx = mix(left, right, t);
        c.fillStyle = flagColors[i % 3]; c.beginPath(); c.moveTo(xx - 9 * z, yy);
        c.lineTo(xx + 9 * z, yy); c.lineTo(xx, yy + 22 * z); c.closePath(); c.fill();
      }
      c.shadowBlur = 0;
      c.strokeStyle = this.skin === "classic" ? "#665c51" : "#64a39c";
      c.lineWidth = (this.skin === "classic" ? 7 : 2) * z;
      for (const [postIndex, [wx, wy]] of (f.variant === 2 || f.appearance?.attachments ? [] : [
        [f.x - f.width / 2, f.y],
        [f.x + f.width / 2, f.y2 ?? f.y],
      ]).entries()) {
        const footX = wx + (f.appearance?.post_offsets[postIndex] ?? 0);
        const xx = sx(wx),
          yy = sy(wy),
          gy = sy(this.surface(s, footX).y);
        c.beginPath();
        c.moveTo(xx, yy);
        c.lineTo(sx(footX), gy);
        c.stroke();
        if (this.skin === "tower") this.sprite("Protector", xx, gy - 10 * z, 29 * z, 0.7);
      }
      c.strokeStyle = "#bfffea";
      c.lineWidth = 1.5 * z;
      for (let i = 0; age === 0 && i < 4; i++) {
        const xx = mix(left, right, (i + 1) / 5),
          yy =
            mix(y, y2, (i + 1) / 5) +
            4 * (f.sag ?? 0) * z * ((i + 1) / 5) * (1 - (i + 1) / 5);
        c.beginPath();
        c.moveTo(xx - 6 * z, yy - 9 * z);
        c.lineTo(xx, yy - 14 * z);
        c.lineTo(xx + 6 * z, yy - 9 * z);
        c.stroke();
      }
    } else if (f.kind === "post") {
      c.strokeStyle = this.skin === "classic" ? "#72675b" : "#628c87";
      c.lineWidth = Math.max(3*z,width); c.lineCap = "round";
      c.beginPath(); c.moveTo(x,y); c.lineTo(x,sy(f.y2??this.surface(s,f.x).y)); c.stroke();
      c.fillStyle="#d6d1b8";c.beginPath();c.arc(x,y,Math.max(3*z,width*.6),0,TAU);c.fill();
    } else if (f.kind === "balloon") {
      const r=width/2, cy=y+r, squash=1-Math.min(0.12,(f.sag??0)*0.7);
      c.save(); c.translate(x,cy); c.scale(1+(1-squash)*0.3,squash);
      const skin=c.createLinearGradient(-r,-r,r,r);
      skin.addColorStop(0,this.skin==="classic"?"#e4ae84":"#99c4ba");
      skin.addColorStop(0.5,this.skin==="classic"?"#b35d58":"#537d89");
      skin.addColorStop(1,this.skin==="classic"?"#743f55":"#273f55");
      c.fillStyle=skin;c.beginPath();c.arc(0,0,r,Math.PI,0);c.bezierCurveTo(r,r*0.7,r*0.35,r*1.15,0,r*1.2);c.bezierCurveTo(-r*0.35,r*1.15,-r,r*0.7,-r,0);c.fill();
      c.strokeStyle="rgba(255,238,205,.35)";c.lineWidth=1.5*z;
      for(const t of [-.66,-.33,0,.33,.66]) {c.beginPath();c.moveTo(0,-r);c.bezierCurveTo(t*r*1.5,-r*.4,t*r*1.5,r*.7,0,r*1.2);c.stroke();}
      c.strokeStyle="#ecd9b1";c.lineWidth=2*z;c.beginPath();c.arc(0,0,r,Math.PI,0);c.stroke();
      c.strokeStyle="#8e8b78";c.beginPath();c.moveTo(-r*.22,r);c.lineTo(-r*.13,r*1.45);c.moveTo(r*.22,r);c.lineTo(r*.13,r*1.45);c.stroke();
      c.fillStyle="#67565c";c.fillRect(-r*.16,r*1.42,r*.32,r*.19);
      if(this.skin==="tower") this.sprite("Protector",0,r*.35,r*.4,.7);
      c.restore();
    } else if (f.kind === "bounce") {
      this.glow(x, y, 50, "#87e4ca", 0.12);
      c.fillStyle = "#28445d";
      c.strokeStyle = "#99e9d6";
      c.lineWidth = 3 * z;
      c.beginPath();
      c.ellipse(x, y + 16 * z, 30 * z, 16 * z, 0, 0, Math.PI * 2);
      c.fill();
      c.stroke();
      this.sprite("Scatter", x, y + 16 * z, 28 * z, 0.9);
      c.strokeStyle = "#c2ffeb";
      c.lineWidth = 2 * z;
      for (let i = 0; i < 2; i++) {
        const yy = y - 18 * z - i * 12 * z - Math.sin(this.elapsed * 3) * 3;
        c.beginPath();
        c.moveTo(x - 7 * z, yy + 5 * z);
        c.lineTo(x, yy);
        c.lineTo(x + 7 * z, yy + 5 * z);
        c.stroke();
      }
    } else if (f.kind === "tank" || f.kind === "ranged" || f.kind === "ray") {
      const ground = sy(this.surface(s, f.x).y);
      c.fillStyle = "#1b2836";
      c.strokeStyle = "#fa8279";
      c.lineWidth = 2 * z;
      c.shadowColor = "#e96d62";
      c.shadowBlur = 8;
      c.beginPath();
      c.moveTo(x - 27 * z, ground);
      c.lineTo(x - 25 * z, y - 25 * z);
      c.lineTo(x - 12 * z, y - 35 * z);
      c.lineTo(x + 20 * z, y - 26 * z);
      c.lineTo(x + 27 * z, ground);
      c.closePath();
      c.fill();
      c.stroke();
      this.sprite(f.kind[0].toUpperCase() + f.kind.slice(1), x, y, 34 * z);
      c.shadowBlur = 0;
      c.strokeStyle = "#ffb0a3";
      c.lineWidth = 2 * z;
      c.beginPath();
      c.moveTo(x - 4 * z, y - 47 * z);
      c.lineTo(x + 4 * z, y - 55 * z);
      c.moveTo(x + 4 * z, y - 47 * z);
      c.lineTo(x - 4 * z, y - 55 * z);
      c.stroke();
    } else if (f.kind === "wall") {
      const top = sy(f.y2 ?? f.y - 1100), left = x - width / 2, right = x + width / 2, height = y - top;
      const shoulderLeft = left - width * 0.17, shoulderRight = right + width * 0.18;
      const footLeft = sy(this.surface(s, f.x - f.width * 0.67).y) + 100 * z;
      const footRight = sy(this.surface(s, f.x + f.width * 0.68).y) + 100 * z;
      const ridge = new Path2D();
      ridge.moveTo(shoulderLeft, footLeft);
      ridge.bezierCurveTo(left - width * 0.08, footLeft - height * 0.2, left - width * 0.1, top + height * 0.25, left, top - height * 0.035);
      ridge.lineTo(left + width * 0.16, top - height * 0.11);
      ridge.quadraticCurveTo(left + width * 0.28, top - height * 0.03, left + width * 0.38, top - height * 0.16);
      ridge.lineTo(left + width * 0.51, top - height * 0.12);
      ridge.lineTo(left + width * 0.65, top - height * 0.20);
      ridge.quadraticCurveTo(left + width * 0.85, top - height * 0.035, right, top - height * 0.045);
      ridge.bezierCurveTo(right + width * 0.075, top + height * 0.12, right + width * 0.08, footRight - height * 0.15, shoulderRight, footRight);
      ridge.lineTo(shoulderRight, Math.max(footRight, y) + 300 * z);
      ridge.lineTo(shoulderLeft, Math.max(footLeft, y) + 300 * z); ridge.closePath();
      const face = c.createLinearGradient(left, top, right, y);
      face.addColorStop(0, this.skin === "classic" ? "#7e8d8c" : "#385366");
      face.addColorStop(0.55, this.skin === "classic" ? "#647977" : "#2a4557");
      face.addColorStop(1, this.skin === "classic" ? "#435e67" : "#183143");
      c.fillStyle = face; c.fill(ridge);
      c.save(); c.clip(ridge);
      // Sedimentary bands and eroded seams replace the old rectangular panel/grid.
      for (let i = 0; i < 22; i++) {
        const u = hash(f.id * 41 + i * 13, s.seed), v = hash(f.id * 17 + i * 29, s.seed);
        const xx = left + u * width, yy = top + v * height;
        const length = width * (0.07 + hash(i * 19 + f.id) * 0.16);
        c.strokeStyle = i % 3 === 0 ? "#d3d9c922" : "#203f471c";
        c.lineWidth = (3 + v * 7) * z;
        c.beginPath(); c.moveTo(xx - length / 2, yy);
        c.quadraticCurveTo(xx, yy + height * (u - 0.5) * 0.10, xx + length / 2, yy - height * 0.02); c.stroke();
      }
      for (let i = 0; i < 9; i++) {
        const u = hash(f.id * 31 + i * 37, s.seed), v = hash(f.id + i * 53, s.seed);
        const xx = left + u * width, yy = top + v * height;
        c.strokeStyle = "#18364022"; c.lineWidth = 3 * z;
        c.beginPath(); c.moveTo(xx, yy);
        c.bezierCurveTo(xx - width * 0.01, yy + height * 0.07, xx + width * 0.02, yy + height * 0.13, xx - width * 0.013, yy + height * 0.23); c.stroke();
      }
      if (this.skin === "tower") for (let i = 0; i < 32; i++)
        this.sprite("Tank", left + ((i % 8 + 0.5) / 8) * width, top + (Math.floor(i / 8) + 0.6) * height / 4, 32 * z, 0.16);
      c.restore();
    } else if (f.kind === "blackhole") {
      const r = 250 * z;
      this.glow(x, y, r, "#a286dc", 0.13);
      c.strokeStyle = "#af9bc8";
      c.lineWidth = 1;
      c.globalAlpha = 0.3;
      for (let i = 0; i < 3; i++) {
        c.beginPath();
        c.arc(
          x,
          y,
          r * (0.4 + i * 0.25),
          this.elapsed * 0.8 + i,
          this.elapsed * 0.8 + i + Math.PI * 1.5,
        );
        c.stroke();
      }
      c.globalAlpha = 1;
      this.glow(x, y, 60 * z, "#9d92cc", 0.45);
      c.fillStyle = "#060f1b";
      c.beginPath();
      c.arc(x, y, 28 * z, 0, Math.PI * 2);
      c.fill();
      c.strokeStyle = "#c8b8f1";
      c.lineWidth = 2 * z;
      c.beginPath();
      c.ellipse(x, y, 44 * z, 17 * z, -0.3, 0, Math.PI * 2);
      c.stroke();
      for (let i = 0; i < 15; i++) {
        const a = this.elapsed * 0.8 + i * 0.79,
          rr = (40 + ((i * 17 + this.elapsed * 40) % 140)) * z;
        c.fillStyle = "#dfc6ff80";
        c.beginPath();
        c.arc(
          x + Math.cos(a) * rr,
          y + Math.sin(a) * rr * 0.65,
          1.5 * z,
          0,
          Math.PI * 2,
        );
        c.fill();
      }
    } else if (f.kind === "chrono") {
      const ground = sy(this.surface(s, f.x).y);
      const top = ground - 260 * z;
      c.fillStyle = "#8cc9e819";
      c.fillRect(x - width / 2, top, width, ground - top);
      c.strokeStyle = "#a8e4e99c";
      c.lineWidth = 1.5 * z;
      c.beginPath();
      c.moveTo(x - width / 2, ground);
      c.lineTo(x - width / 2, top);
      c.quadraticCurveTo(x, top - 40 * z, x + width / 2, top);
      c.lineTo(x + width / 2, ground);
      c.stroke();
      for (let i = 0; i < 7; i++) {
        const yy = ground - ((this.elapsed * 14 + i * 35) % 240) * z;
        c.strokeStyle = "#c2f1e537";
        c.beginPath();
        c.moveTo(x - width / 2 + 10 * z, yy);
        c.quadraticCurveTo(x, yy - 15 * z, x + width / 2 - 10 * z, yy);
        c.stroke();
      }
      this.sprite("Chrono Field", x, ground - 20 * z, 30 * z, 0.75);
    } else if (f.kind === "deathwave") {
      const waveX = f.x + Math.sin(s.time * 2 + f.id) * 24;
      const xx = sx(waveX),
        height = 38 + 12 * Math.sin(s.time * 3 + f.id);
      const top = sy(this.surface(s, waveX).y - height);
      this.glow(xx, top, 65 * z, "#f88f9d", 0.15);
      c.strokeStyle = "#ffacaf";
      c.fillStyle = "#e16e7b55";
      c.lineWidth = 2.5 * z;
      c.beginPath();
      for (let i = 0; i <= 12; i++) {
        const wx = waveX - f.width / 2 + (f.width * i) / 12;
        const yy = sy(this.surface(s, wx).y - height);
        if (i === 0) c.moveTo(sx(wx), yy);
        else c.lineTo(sx(wx), yy);
      }
      for (let i = 12; i >= 0; i--) {
        const wx = waveX - f.width / 2 + (f.width * i) / 12;
        c.lineTo(sx(wx), sy(this.surface(s, wx).y));
      }
      c.closePath();
      c.fill();
      c.stroke();
      c.lineWidth = 2 * z;
      c.beginPath();
      c.moveTo(xx - 5 * z, top - 10 * z);
      c.lineTo(xx + 5 * z, top - 20 * z);
      c.moveTo(xx + 5 * z, top - 10 * z);
      c.lineTo(xx - 5 * z, top - 20 * z);
      c.stroke();
    } else if (f.kind === "swamp") {
      const ground = sy(this.surface(s, f.x).y);
      c.fillStyle = "#7cb47333";
      c.fillRect(x - width / 2, ground - 4 * z, width, 19 * z);
      c.strokeStyle = "#c2e795";
      c.lineWidth = 3 * z;
      c.beginPath();
      c.moveTo(x - width / 2, ground);
      for (let i = 0; i <= 12; i++)
        c.lineTo(
          x - width / 2 + (width * i) / 12,
          ground + Math.sin(i + this.elapsed * 3) * 2 * z,
        );
      c.stroke();
      for (let i = 0; i < 7; i++) {
        const xx = x - width / 2 + (width * (i + 0.5)) / 7,
          yy = ground - ((this.elapsed * 18 + i * 13) % 55) * z;
        c.fillStyle = "#bbd9985a";
        c.beginPath();
        c.arc(xx, yy, (2 + (i % 3)) * z, 0, Math.PI * 2);
        c.fill();
      }
    } else if (f.kind === "lightning") {
      const active = phase >= 0.62 && phase < 0.9,
        warning = phase >= 0.35 && phase < 0.62,
        arc = y - 110 * z;
      c.strokeStyle = active ? "#e3f5ff" : warning ? "#8fcbd199" : "#8fcbd12a";
      c.lineWidth = (active ? 3 : 1) * z;
      c.shadowColor = "#75cfff";
      c.shadowBlur = active ? 15 : 0;
      c.beginPath();
      for (let i = 0; i < 9; i++) {
        const xx = x - width / 2 + (width * i) / 8,
          yy = arc + (i % 2 === 0 ? -1 : 1) * (active ? 12 : 4) * z;
        if (i === 0) c.moveTo(xx, yy);
        else c.lineTo(xx, yy);
      }
      c.stroke();
      for (const xx of [x - width / 2, x + width / 2])
        this.sprite("Ray", xx, arc, 23 * z, 0.65);
      if (warning) {
        c.strokeStyle = "#a4e2f0";
        c.lineWidth = 1.5 * z;
        c.beginPath();
        c.moveTo(x - 8 * z, arc + 25 * z);
        c.lineTo(x, arc + 33 * z);
        c.lineTo(x + 8 * z, arc + 25 * z);
        c.stroke();
      }
    } else if (f.kind === "spotlight") {
      const active = phase < 0.65,
        ground = sy(this.surface(s, f.x).y),
        center = x + Math.sin(s.time * 1.4 + f.id) * 65 * z;
      c.globalAlpha = active ? 0.27 : 0.055;
      const g = c.createLinearGradient(tx, ty, center, ground);
      g.addColorStop(0, "#ffeac200");
      g.addColorStop(1, "#ffdea7");
      c.fillStyle = g;
      c.beginPath();
      c.moveTo(tx, ty);
      c.lineTo(center - width * 0.36, ground + 12 * z);
      c.lineTo(center + width * 0.36, ground + 12 * z);
      c.closePath();
      c.fill();
      c.globalAlpha = active ? 0.7 : 0.3;
      c.strokeStyle = "#ffe1a7";
      c.lineWidth = 2 * z;
      c.beginPath();
      c.ellipse(center, ground, width * 0.36, 5 * z, 0, 0, Math.PI * 2);
      c.stroke();
    }
    c.restore();
  }
}
