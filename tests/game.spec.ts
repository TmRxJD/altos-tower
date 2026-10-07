import { test, expect } from "@playwright/test";
test("Felipe double jump banks a named backflip and immediate boost", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("altos-tower-v1", JSON.stringify({ save_version: 5, meters: 100000, coins: 300, flips: 60, missions: { level: 51, values: [0, 0, 0], completed: [false, false, false], history: Array.from({ length: 50 }, (_, i) => i + 1) } }));
    Object.defineProperty(crypto, "getRandomValues", { configurable: true, value: (values: Uint32Array) => { values[0] = 1; return values; } });
  });
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#roster-open").click();
  await page.locator('[data-rider="5"]').click();
  await page.locator("#roster-panel .close-panel").click();
  await page.locator("#start").click();
  await expect(page.locator("#scene")).toHaveAttribute("data-ruleset", "6");
  const before = Number(await page.locator("#scene").getAttribute("data-speed"));
  await page.keyboard.down("Space");
  await page.waitForTimeout(500);
  await page.keyboard.up("Space");
  await page.keyboard.down("Space");
  await page.waitForFunction(() => document.querySelector("#combo")?.textContent?.includes("Backflip"), undefined, { polling: "raf", timeout: 2500 });
  // The original queues the trick while inverted; release after the pose has
  // completed the rotation so the separate landing check succeeds.
  await page.waitForFunction(() => Number(document.querySelector<HTMLElement>("#scene")?.dataset.poseAngle ?? 0) < -Math.PI * 2 + 0.1, undefined, { polling: "raf", timeout: 2500 });
  await page.keyboard.up("Space");
  await expect(page.locator("#notice")).not.toContainText("Night flight");
  await expect(page.locator("#combo")).toContainText("Backflip", { timeout: 3000 });
  await expect(page.locator("#combo")).toHaveClass(/banked/, { timeout: 10000 });
  expect(Number(await page.locator("#scene").getAttribute("data-speed"))).toBeGreaterThan(before * 1.2);
  expect(Number(await page.locator("#scene").getAttribute("data-boost"))).toBeGreaterThan(0);
  await expect(page.locator(".boost-status")).toContainText("BOOST");
  await expect(page.locator(".boost-status")).toContainText(/≈\d+ m/);
  await expect(page.locator("#overlay")).toBeHidden();
  await page.screenshot({ path: "test-results/flip-boost.png" });
  await page.waitForFunction(() => Number(document.querySelector<HTMLElement>("#scene")?.dataset.boost ?? 0) === 0, undefined, { polling: "raf", timeout: 5000 });
  await expect(page.locator(".boost-status")).toHaveCount(0);
});
test("descent, ability, pause, roster and persistence", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("response", (r) => {
    if (r.status() >= 400) errors.push(`${r.status()} ${r.url()}`);
  });
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#roster-open").click();
  await expect(page.getByRole("heading", { name: "Characters" })).toBeVisible();
  await expect(page.locator('[data-rider="0"]')).toBeEnabled();
  await expect(page.locator('[data-rider="1"]')).toBeDisabled();
  await expect(page.locator('[data-rider="5"]')).toContainText("Complete level 40");
  await page.locator("#roster-panel .close-panel").click();
  await page.getByRole("button", { name: "Controls" }).click();
  await expect(page.getByRole("heading", { name: "Controls" })).toBeVisible();
  await page.locator("#guide-panel .close-panel").click();
  await page.getByRole("button", { name: "Zen", exact: true }).click();
  await expect(page.locator("#hud")).toBeVisible();
  await expect(page.locator("#ability")).toHaveCount(0);
  await expect(page.locator("#wing")).toBeVisible();
  await expect(page.locator("#wing")).toBeDisabled();
  await page.keyboard.press("Space");
  await page.waitForTimeout(1600);
  const d = await page.locator("#distance").innerText();
  expect(parseInt(d)).toBeGreaterThan(0);
  await page.locator("#pause").click();
  await expect(page.getByRole("heading", { name: "Paused" })).toBeVisible();
  const paused = await page.locator("#distance").innerText();
  await page.waitForTimeout(300);
  await expect(page.locator("#distance")).toHaveText(paused);
  await page.locator("#resume").click();
  await page.waitForTimeout(400);
  expect(parseInt(await page.locator("#distance").innerText())).toBeGreaterThan(
    parseInt(paused),
  );
  await page.locator("#pause").click();
  await page.locator("#home").click();
  const before = await page.evaluate(
    () => JSON.parse(localStorage.getItem("altos-tower-v1")!).meters,
  );
  expect(before).toBeGreaterThan(0);
  await page.reload();
  await expect(page.locator("#loading")).toBeHidden();
  expect(
    await page.evaluate(
      () => JSON.parse(localStorage.getItem("altos-tower-v1")!).meters,
    ),
  ).toBeGreaterThanOrEqual(before);
  expect(errors).toEqual([]);
});
test("unlocks restore and touch controls fit mobile", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(() =>
    localStorage.setItem(
      "altos-tower-v1",
      JSON.stringify({ save_version: 5, meters: 100000, coins: 300, flips: 60, missions: { level: 51, values: [0, 0, 0], completed: [false, false, false], history: Array.from({ length: 50 }, (_, i) => i + 1) } }),
    ),
  );
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#roster-open").click();
  await page.locator('[data-rider="5"]').click();
  await expect(page.locator('[data-rider="5"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.locator("#roster-panel .close-panel").click();
  await page.locator("#zen").click();
  await expect(page.locator("#ability")).toHaveCount(0);
  await expect(page.locator("#wing")).toBeVisible();
  await expect(page.locator("#wing")).toBeDisabled();
  for (const id of ["jump", "wing", "pause"]) {
    const box = await page.locator("#" + id).boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(390);
    expect(box!.height).toBeGreaterThanOrEqual(44);
  }
  await page.locator("#jump").click();
  await page.screenshot({ path: "test-results/mobile-play.png" });
});
test("capture title and roster", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await page.screenshot({ path: "test-results/title.png" });
  await page.locator("#roster-open").click();
  await page.screenshot({ path: "test-results/roster.png" });
  await page.locator("#roster-panel .close-panel").click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator("#guide-open").click();
  await expect(page.locator("#guide-panel")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Controls" })).toBeVisible();
  await page.screenshot({ path: "test-results/controls.png" });
  await page.locator("#guide-panel .close-panel").click();
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.locator("#zen").click();
  await expect(page.locator("#hud")).toBeVisible();
  await page.waitForTimeout(10_000);
  await page.screenshot({ path: "test-results/dynamic-horde.png" });
  await page.locator("#pause").click();
  await expect(page.locator("#dialog-title")).toHaveText("Paused");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: "test-results/mobilepause.png" });
});
test("classic and Tower skins switch from the menu", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    Object.defineProperty(crypto, "getRandomValues", {
      configurable: true,
      value: (values: Uint32Array) => {
        values[0] = 1;
        return values;
      },
    });
  });
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await expect(
    page.getByRole("button", { name: "Switch to Tower" }),
  ).toBeVisible();
  await page.screenshot({ path: "test-results/classic-menu.png" });
  await page.locator("#start").click();
  await expect(page.locator("#hud")).toBeVisible();
  await page.screenshot({ path: "test-results/classic-play.png" });
  await page.locator("#pause").click();
  await page.locator("#home").click();
  await page.getByRole("button", { name: "Switch to Tower" }).click();
  await expect(
    page.getByRole("button", { name: "Switch to Classic" }),
  ).toBeVisible();
  await page.screenshot({ path: "test-results/tower-menu.png" });
  await page.getByRole("button", { name: "Switch to Classic" }).click();
  await expect(
    page.getByRole("button", { name: "Switch to Tower" }),
  ).toBeVisible();
});

test("bad landing shows results and retry starts a new seed", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    let seedCalls = 0;
    Object.defineProperty(crypto, "getRandomValues", {
      configurable: true,
      value: (values: Uint32Array) => {
        values[0] = seedCalls++ === 0 ? 1 : 43;
        return values;
      },
    });
  });
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#start").click();
  await expect(page.locator("#hud")).toBeVisible();
  const firstSeed = await page.locator("#scene").getAttribute("data-run-seed");
  expect(firstSeed).toBe("1");
  await page.screenshot({ path: "test-results/opening-play.png" });
  await page.keyboard.press("Space");
  await page.waitForTimeout(300);
  await page.keyboard.down("Space");
  await expect(page.locator("#dialog-title")).toHaveText("Run over", {
    timeout: 8000,
  });
  await page.keyboard.up("Space");
  await expect(page.locator("#dialog-detail")).toContainText("Bad landing");
  await expect(page.locator("#result")).toContainText("coins");
  await page.screenshot({ path: "test-results/results.png" });
  await page.locator("#retry").click();
  await expect(page.locator("#overlay")).toBeHidden();
  const retrySeed = await page.locator("#scene").getAttribute("data-run-seed");
  expect(retrySeed).not.toBe(firstSeed);
  const restartedMeters = Number(
    (await page.locator("#distance").innerText()).replace(/[^\d]/g, ""),
  );
  expect(restartedMeters).toBeLessThan(10);
});

test("long grind builds the scarf without premature wingsuit activation", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    Object.defineProperty(crypto, "getRandomValues", {
      configurable: true,
      value: (values: Uint32Array) => {
        values[0] = 1;
        return values;
      },
    });
  });
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#start").click();
  await expect(page.locator("#scene")).toHaveAttribute("data-run-seed", "1");
  await expect(page.locator("#hud")).toBeVisible();
  await page.screenshot({ path: "test-results/smooth-background.png" });
  const takeoff = await page.evaluate(async () => {
    const path = "/src/wasm/engine.js"; const {default:init,Game}=await import(path);await init();
    const probe=new Game("{}");probe.start_seeded(false,1);const state=JSON.parse(probe.snapshot());probe.free();
    const rail=state.features.find((f:any)=>f.kind==="rail"&&f.variant===0);
    if(!rail)throw new Error("No opening bunting for this seeded jump test");
    return rail.x-rail.width/2-600;
  });
  await expect
    .poll(
      async () =>
        Number(await page.locator("#scene").getAttribute("data-world-x")),
      { timeout: 5000, intervals: [16] },
    )
    .toBeGreaterThanOrEqual(takeoff);
  await page.keyboard.press("Space");
  await expect(page.locator("#scene")).toHaveAttribute("data-motion", "grind", { timeout: 3000 });
  await expect(page.locator("#combo")).toContainText("Bunting grind");
  await expect(page.locator("#wing .wing-status")).toHaveText(/^[1-9]\d*%$/, {timeout:10000});
  await expect(page.locator("#wing")).toBeDisabled();
  await expect(page.locator("#hud")).toBeVisible();
  await expect(page.locator("#overlay")).toBeHidden();
  await page.screenshot({path:"test-results/scarf-charge.png"});
});
