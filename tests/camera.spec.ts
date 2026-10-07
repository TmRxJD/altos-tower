import { test, expect } from "@playwright/test";
import { FollowCamera, PLAY_ZOOM, terrainFraming } from "../src/camera";

test("reference framing and camera lag remain stable across display refresh rates", () => {
  // Native landmark / HUD estimate is approximately 10–12px per meter at 540px.
  expect(PLAY_ZOOM * (540 / 900) * 40).toBeCloseTo(10.8, 5);
  const results: FollowCamera[] = [];
  for (const hz of [30, 60, 144]) {
    const camera = new FollowCamera();
    camera.reset(0, 0);
    for (let i = 1; i <= hz * 2; i++)
      camera.update(i / hz * 1200, -i / hz * 500, 1200, false, 1 / hz);
    results.push(camera);
  }
  for (const camera of results) {
    expect(camera.x).toBeCloseTo(results[0].x, 7);
    expect(camera.y).toBeCloseTo(results[0].y, 7);
    // Follow the actual airborne rider; a floor-following camera loses tall jumps.
    expect(-1000 - camera.y).toBeGreaterThan(-120);
    expect(-1000 - camera.y).toBeLessThan(0);
  }
  const camera = results[0], x = camera.x, y = camera.y;
  camera.update(10000, 10000, 1200, true, 0);
  expect(camera.x).toBe(x);
  expect(camera.y).toBe(y);
});

test("takeoff and canyon contact changes do not brake the horizontal camera", () => {
  const constant = new FollowCamera(), transitions = new FollowCamera();
  constant.reset(0, 0); transitions.reset(0, 0);
  for (let i = 1; i <= 240; i++) {
    const x = i * 800 / 120;
    constant.update(x, 0, 800, true, 1 / 120);
    transitions.update(x, 0, 800, i < 30 || (i > 160 && i % 2 === 0), 1 / 120);
    expect(transitions.x).toBeCloseTo(constant.x, 9);
  }
});

test("steep framing preserves a nearby trailing face without extreme zoom", () => {
  for (const width of [416, 1600, 2200]) {
    const normal = terrainFraming(0.2, width, 900);
    expect(normal.zoom).toBe(PLAY_ZOOM);
    expect(normal.playerX).toBe(width * 0.2);
    const steep = terrainFraming(2.2, width, 900);
    // Brief steep faces leave a visible shoulder; long sustained plunges cannot
    // be solved by fitting their entire summit into the screen.
    const behind = width < 500 ? 250 : 600;
    const crestX = steep.playerX - behind * steep.zoom;
    const crestY = steep.baseline - behind * 1.5 * steep.zoom;
    expect(crestX).toBeGreaterThan(0);
    expect(crestX).toBeLessThan(width);
    expect(crestY).toBeGreaterThan(0);
    expect(steep.baseline).toBeLessThan(750);
    for (const slope of [0.2, 1, 2.2, 3.6, 100]) {
      expect(terrainFraming(slope, width, 900).zoom).toBeGreaterThanOrEqual(PLAY_ZOOM * 0.72);
    }
  }
});

test("generated steep face retains trailing terrain at a bounded scale", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  for (const width of [1440, 416]) {
    await page.setViewportSize({ width, height: 900 });
    const frame = await page.evaluate(async () => {
      const enginePath = "/src/wasm/engine.js", scenePath = "/src/scene.ts";
      const { default: init, Game } = await import(enginePath);
      await init();
      const { Scene } = await import(scenePath);
      let s, face;
      for (let seed = 1; seed <= 64; seed++) {
        const game = new Game("{}"); game.start_seeded(true, seed);
        const state = JSON.parse(game.snapshot()); game.free();
        const segment = state.terrain_segments.find((p: {m1: number}) => p.m1 > 1.8);
        if (segment) { s = state; face = segment; break; }
      }
      if (!face) throw new Error("No seeded steep face found");
      s.x = face.x1; s.y = face.y1 - 18; s.angle = Math.atan(face.m1); s.vx = 0; s.vy = 0;
      // Rendering fixture only: the pursuer is genuinely offscreen, so the
      // distance indicator should appear without clamping its world position.
      s.chase = { x: s.x - 3000, y: s.y, previous_x: s.x - 3000, previous_y: s.y, speed: 800 };
      const canvas = document.createElement("canvas");
      canvas.id = "framing-fixture";
      canvas.style.cssText = "position:fixed;inset:0;width:100vw;height:100vh;z-index:1000";
      document.getElementById(canvas.id)?.remove(); document.body.append(canvas);
      const scene = new Scene(canvas, {});
      const labels: string[] = [];
      const ctx = canvas.getContext("2d")!;
      const fillText = ctx.fillText.bind(ctx);
      ctx.fillText = (text: string, x: number, y: number, maxWidth?: number) => {
        labels.push(text);
        if (maxWidth === undefined) fillText(text, x, y);
        else fillText(text, x, y, maxWidth);
      };
      // Let the initial ResizeObserver delivery settle before drawing the
      // fixture; changing canvas dimensions clears its pixel buffer.
      await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
      for (let i = 0; i < 240; i++) scene.render(s, "play", 1 / 60);
      const behind = Math.min(600, scene.width * scene.riderFrameX / scene.zoom * 0.75);
      const terrain = scene.surface(s, s.x - behind);
      return {
        zoom: scene.zoom,
        painted: canvas.getContext("2d")!.getImageData(0, 0, 1, 1).data[3],
        pursuitIndicator: labels.includes("‹ 75 m"),
        terrainX: scene.width * scene.riderFrameX - behind * scene.zoom,
        terrainY: (terrain.y - scene.camY) * scene.zoom + 900 * scene.riderFrameY,
      };
    });
    expect(frame.zoom).toBeGreaterThanOrEqual(PLAY_ZOOM * 0.72);
    expect(frame.painted).toBe(255);
    expect(frame.pursuitIndicator).toBe(true);
    expect(frame.terrainX).toBeGreaterThan(0);
    expect(frame.terrainY).toBeGreaterThan(0);
    expect(frame.terrainY).toBeLessThan(900);
    await page.screenshot({ path: `test-results/steep-framing-${width}.png` });
  }
});

test("recorded village framing settles gradually and survives cancelling the launch pose", async ({ page }) => {
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  const frames = await page.evaluate(async () => {
    const enginePath = "/src/wasm/engine.js", scenePath = "/src/scene.ts";
    const { default: init, Game } = await import(enginePath); await init();
    const { Scene } = await import(scenePath);
    const game = new Game("{}"); game.start_seeded(true, 2);
    const base = JSON.parse(game.snapshot()); game.free();
    const results = [];
    for (const hz of [30,60,144]) {
      const canvas = document.createElement("canvas"); canvas.style.cssText = "position:fixed;inset:0;width:100vw;height:100vh";
      document.body.append(canvas); const scene = new Scene(canvas, {});
      await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
      scene.render(base,"menu",0); scene.beginEntrance(true);
      let early;
      for (let i=1;i<=hz*2.5;i++) {
        const state = { ...base, x: i/hz*900, vx:900, y:scene.surface(base,i/hz*900).y-18 };
        if (i/hz >= .2) scene.presentation.clear(); // Jump stops the pose only.
        scene.render(state,"play",1/hz);
        if (i===hz/2) early={ x:scene.riderFrameX, y:scene.riderFrameY, zoom:scene.zoom };
      }
      results.push({ early, late:{ x:scene.riderFrameX, y:scene.riderFrameY, zoom:scene.zoom } }); canvas.remove();
    }
    return results;
  });
  for (const f of frames) {
    expect(f.early!.x).toBeGreaterThan(.48); expect(f.early!.y).toBeGreaterThan(.70);
    expect(f.late.x).toBeCloseTo(.2,4); expect(f.late.zoom).toBeCloseTo(PLAY_ZOOM,4);
  }
  expect(Math.max(...frames.map(f=>f.early!.x))-Math.min(...frames.map(f=>f.early!.x))).toBeLessThan(.002);
});
