import { test, expect } from "@playwright/test";
import { RunPresentation, footing, mountPose } from "../src/presentation";
import type { State } from "../src/types";

test("title rider stands beside the board, hops onto it, and compresses on contact", () => {
  const idle = mountPose(true, null, 0);
  expect(Math.abs(idle.x-idle.boardX)).toBeGreaterThan(30);
  const anticipation=mountPose(false,"intro",.18), airborne=mountPose(false,"intro",.47), contact=mountPose(false,"intro",.75);
  expect(airborne.y).toBeLessThan(anticipation.y-30);
  expect(Math.abs(airborne.x-airborne.boardX)).toBeLessThan(Math.abs(idle.x-idle.boardX));
  expect(contact.x).toBe(0); expect(contact.boardX).toBe(0); expect(contact.compression).toBeGreaterThan(8);
  expect(mountPose(false,"intro",1.2).compression).toBeLessThan(.1);
  expect(mountPose(false,null,.47).y).toBe(0); // Jump cancels the cosmetic hop immediately.
  expect(mountPose(true,"intro",.47,true).y).toBe(0);
});

test("presentation clocks are frame independent and foundations sample the full slope", () => {
  const times = [30, 60, 144].map(hz => {
    const p = new RunPresentation(); p.begin("intro");
    for (let i = 0; i < hz; i++) p.advance(1 / hz);
    return p.simulationRate;
  });
  expect(Math.max(...times) - Math.min(...times)).toBeLessThan(1e-10);
  const feet = footing(100, 200, x => .4 * x + .0005 * x * x);
  expect(feet[0]).toEqual({ x: 0, y: 0 });
  expect(feet.at(-1)).toEqual({ x: 200, y: 100 });
  expect(feet[6].y).toBe(45);
  const p = new RunPresentation(); p.begin("crash"); p.advance(.4);
  const state = { x: 0, y: -18, angle: 0, vx: 600, vy: 0, notice: "Collision" } as State;
  const pose = p.pose(state, () => ({ y: 0, gap: false }));
  expect(pose.x).toBeGreaterThan(state.x); expect(pose.angle).toBeGreaterThan(0);
  expect(state).toEqual({ x: 0, y: -18, angle: 0, vx: 600, vy: 0, notice: "Collision" });
  const reduced = new RunPresentation(true); reduced.begin("crash");
  expect(reduced.advance(.2)).toBe(true);
});

test("entrance eases in, pauses, and accepts Jump without a lockout", async ({ page }) => {
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#start").click();
  await expect(page.locator("#scene")).toHaveAttribute("data-presentation", "intro");
  await page.screenshot({ path: "test-results/entrance.png" });
  await page.keyboard.press("KeyP");
  await expect(page.locator("#dialog-title")).toHaveText("Paused");
  const time = await page.locator("#scene").getAttribute("data-presentation-time");
  await page.waitForTimeout(300);
  expect(await page.locator("#scene").getAttribute("data-presentation-time")).toBe(time);
  await page.keyboard.press("KeyP");
  await page.keyboard.down("Space");
  await expect(page.locator("#scene")).toHaveAttribute("data-presentation", "ride");
  await expect(page.locator("#scene")).toHaveAttribute("data-motion", "air");
  await page.keyboard.up("Space");
});

test("death tumbles before results and pause freezes the crash", async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(crypto, "getRandomValues", { configurable: true, value: (values: Uint32Array) => { values[0] = 42; return values; } }));
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#start").click();
  await page.keyboard.press("Space");
  await page.waitForTimeout(700); await page.keyboard.down("Space");
  await expect(page.locator("#scene")).toHaveAttribute("data-presentation", "crash", { timeout: 4000 });
  await expect(page.locator("#overlay")).toBeHidden();
  await page.waitForTimeout(180);
  await page.screenshot({ path: "test-results/crash.png" });
  await page.keyboard.press("KeyP");
  const time = await page.locator("#scene").getAttribute("data-presentation-time");
  await page.waitForTimeout(400);
  expect(await page.locator("#scene").getAttribute("data-presentation-time")).toBe(time);
  await page.keyboard.up("Space"); await page.keyboard.press("KeyP");
  await expect(page.locator("#dialog-title")).toHaveText("Run over", { timeout: 3000 });
});

test("hillside facades stay seated and reduced-motion crashes use a static pose", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  await page.evaluate(async () => {
    const enginePath = "/src/wasm/engine.js", scenePath = "/src/scene.ts";
    const { default: init, Game } = await import(enginePath); await init();
    const { Scene } = await import(scenePath);
    const game = new Game("{}"); game.start_seeded(true, 2);
    const s = JSON.parse(game.snapshot()); game.free();
    s.x = 1000; s.y = 182; s.angle = Math.atan(.3); s.speed = 0; s.vx = 0; s.vy = 0; s.chase = null; s.camp = null; s.gaps = [];
    s.terrain_segments = [{ x0: -5000, x1: 6000, y0: -1600, y1: 1700, m0: .3, m1: .3 }];
    s.features = [{ id: 1, kind: "hut", x: 1600, y: 380, width: 360, active: true }, { id: 2, kind: "ruin", x: 2450, y: 635, width: 250, active: true }, { id: 3, kind: "rail", variant: 1, x: 3250, y: 415, y2: 605, width: 700, active: true }];
    const canvas = document.createElement("canvas"); canvas.id = "scenery-fixture";
    canvas.style.cssText = "position:fixed;inset:0;width:100vw;height:100vh;z-index:1000"; document.body.append(canvas);
    const scene = new Scene(canvas, {});
    // Let the newly inserted canvas finish its initial ResizeObserver delivery
    // before drawing and capturing the rendering fixture.
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    scene.render(s, "play", 0);
    const ctx = canvas.getContext("2d")!;
    // Downhill corner below the old center-anchored flat bottom must be plaster,
    // rather than the sky color visible in the user's floating-house example.
    const sx = (x: number) => (x - scene.camX) * scene.zoom + 1600 * scene.riderFrameX;
    const sy = (y: number) => (y - scene.camY) * scene.zoom + 900 * scene.riderFrameY;
    const pixel = ctx.getImageData(Math.round(sx(1760) * .8), Math.round(sy(412) * .8), 1, 1).data;
    if (pixel[0] < 100) throw new Error(`Unseated hillside facade: ${[...pixel]}`);
  });
  await page.screenshot({ path: "test-results/scenery-alignment.png" });
});

// These fixtures verify presentation state transitions rather than native physics.
test("Play launches the title world and Retry generates a fresh world", async ({ page }) => {
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  const titleSeed = await page.locator("#scene").getAttribute("data-run-seed");
  await page.screenshot({ path: "test-results/title-village.png" });
  await page.locator("#start").click();
  await expect(page.locator("#scene")).toHaveAttribute("data-run-seed", titleSeed!);
  await page.waitForTimeout(350);
  await page.screenshot({ path: "test-results/village-kickoff.png" });
  await page.keyboard.press("KeyP"); await page.locator("#retry").click();
  await expect(page.locator("#scene")).not.toHaveAttribute("data-run-seed", titleSeed!);
});

test("cloth flight and ridden bunting survive rendering with a broken cable", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  const result = await page.evaluate(async () => {
    const enginePath = "/src/wasm/engine.js", scenePath = "/src/scene.ts";
    const { default: init, Game } = await import(enginePath); await init();
    const { Scene } = await import(scenePath);
    const game = new Game("{}"); game.start_seeded(true, 2);
    const s = JSON.parse(game.snapshot()); game.free();
    s.x = 5000; s.y = 180; s.angle = -.2; s.speed = 0; s.vx = 0; s.vy = 0; s.chase = null; s.camp = null; s.gaps = []; s.rider = 0;
    s.terrain_segments = [{ x0: -5000, x1: 12000, y0: 700, y1: 700, m0: 0, m1: 0 }];
    s.features = [{ id: 31, kind: "rail", variant: 0, x: 5800, y: 200, y2: 260, sag: 100, width: 1600, active: true }];
    s.rope_states = { 31: { load: .2, broken_at: null, ridden_to: 5200 } };
    s.wing_on = true; s.grounded = false; s.rail = null;
    const canvas = document.createElement("canvas"); canvas.id = "flight-fixture";
    canvas.style.cssText = "position:fixed;inset:0;width:100vw;height:100vh;z-index:1000"; document.body.append(canvas);
    const scene = new Scene(canvas, {});
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    scene.render(s, "play", 0);
    s.rope_states[31].ridden_to = 5700; scene.render(s, "play", 0);
    const internals = scene as unknown as { particles: {kind?: string; x:number}[]; ropeBreaks: Map<number,number> };
    const flags = internals.particles.filter(p => p.kind === "flag");
    s.features[0].active = false; s.rope_states[31].broken_at = s.time;
    scene.render(s, "play", .35);
    scene.render(s, "play", .35);
    return { count: flags.length, positions: flags.map(f => f.x), brokenVisible: internals.ropeBreaks.has(31) };
  });
  expect(result.count).toBe(5); expect(result.positions.every(x => x > 5200 && x <= 5700)).toBe(true);
  expect(result.brokenVisible).toBe(true);
  await page.screenshot({ path: "test-results/cloth-flight-bunting.png" });
});

test("seeded wall encounters expose distinct visible transfer routes", async ({ page }) => {
  await page.setViewportSize({ width: 2000, height: 720 });
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  const routes = await page.evaluate(async () => {
    const enginePath = "/src/wasm/engine.js";
    const { default: init, Game } = await import(enginePath); await init();
    const found: Record<number, {state: any; seed:number}> = {};
    for(let seed=1;seed<=256 && Object.keys(found).length<4;seed++) {
      const game = new Game("{}"); game.start_seeded(true,seed);
      const state=JSON.parse(game.snapshot()); game.free();
      const wall=state.features.find((f:any)=>f.kind==="wall" && f.variant<=3);
      if(wall && !found[wall.variant]) found[wall.variant]={state,seed};
    }
    (window as any).wallRoutes = found;
    return Object.entries(found).map(([variant,{state,seed}])=>({variant:Number(variant),seed, topology:state.features.filter((f:any)=>["wall","balloon","rail"].includes(f.kind)).map((f:any)=>`${f.kind}:${f.variant??0}`).join(",")}));
  });
  expect(routes).toHaveLength(4); expect(new Set(routes.map(r=>r.topology)).size).toBe(4);
  for (const route of routes) {
    await page.evaluate(async (variant) => {
      const scenePath="/src/scene.ts"; const { Scene }=await import(scenePath);
      const s=(window as any).wallRoutes[variant].state;
      const f=s.features.find((f:any)=>f.kind==="wall" && f.variant===variant);
      s.x=f.x-f.width/2+400; s.y=f.y2+750; s.grounded=false; s.wall=f.id; s.angle=-.2; s.speed=0; s.vx=0; s.vy=0; s.chase=null; s.camp=null;
      document.getElementById("wall-fixture")?.remove();
      const canvas=document.createElement("canvas"); canvas.id="wall-fixture"; canvas.style.cssText="position:fixed;inset:0;width:100vw;height:100vh;z-index:1000"; document.body.append(canvas);
      const scene=new Scene(canvas,{});
      await new Promise<void>(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve())));
      scene.render(s,"play",0);
    },route.variant);
    await page.screenshot({ path:`test-results/wall-route-${route.variant}.png` });
  }
});
