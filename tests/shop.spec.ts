import { test, expect } from "@playwright/test";

test("shop spends coins, applies upgrades and preserves progression across reload", async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem("altos-tower-v1")) localStorage.setItem("altos-tower-v1", JSON.stringify({ save_version: 2, coins: 2500, meters: 900, flips: 5 }));
  });
  const errors: string[] = []; page.on("pageerror", e => errors.push(e.message));
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  await expect(page.locator("#wallet-total")).toHaveText("2,500");
  await page.locator("#shop-open").click();
  await expect(page.getByRole("dialog", { name: "Shop" })).toBeVisible();
  await page.getByRole("button", { name: "Upgrade Magnet", exact: true }).click();
  await expect(page.locator("#shop-wallet")).toHaveText("2,000");
  await expect(page.locator('[data-item="magnet"]')).toContainText("7.5s → 10s");
  await page.getByRole("button", { name: "Buy Helmet", exact: true }).click();
  await expect(page.locator("#shop-wallet")).toHaveText("500");
  await expect(page.locator('[data-item="helmet"]')).toContainText("1 owned");
  await expect(page.getByRole("button", { name: "Upgrade Wingsuit lining" })).toBeDisabled();
  await expect(page.locator('[data-item="wingsuit"]')).toContainText("Need 1,000 more");
  await page.keyboard.press("Escape"); await expect(page.locator("#shop-panel")).not.toBeVisible();
  await expect(page.locator("#start")).toBeVisible();
  await page.reload(); await expect(page.locator("#loading")).toBeHidden();
  await expect(page.locator("#wallet-total")).toHaveText("500");
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("altos-tower-v1")!));
  expect(saved.coins).toBe(2500); expect(saved.upgrades.magnet).toBe(1); expect(saved.helmets).toBe(1);
  await page.locator("#start").click(); await expect(page.locator("#buffs")).toContainText("Shield");
  expect(errors).toEqual([]);
});

test("shop fits a narrow screen and stays keyboard accessible", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden(); await page.locator("#shop-open").click();
  await expect(page.locator("#shop-close")).toBeFocused();
  const panel = await page.locator("#shop-panel").boundingBox();
  expect(panel!.x).toBeGreaterThanOrEqual(0); expect(panel!.x + panel!.width).toBeLessThanOrEqual(390);
  expect(await page.locator("#shop-panel").evaluate(e => e.scrollWidth <= e.clientWidth)).toBe(true);
  await page.screenshot({ path: "test-results/shop-mobile.png" });
  await page.keyboard.press("Escape"); await expect(page.locator("#shop-open")).toBeFocused();
});

test("wealth cannot bypass level gates or the post-campaign final tier", async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem("altos-tower-v1")) localStorage.setItem("altos-tower-v1", JSON.stringify({ save_version: 5, coins: 1000000, wallet: 1000000, missions: { level: 1, values: [0, 0, 0], completed: [false, false, false], history: [] }, upgrades: { magnet: 1, feather: 0, wingsuit: 0, scarf: 0 } }));
  });
  await page.goto("/"); await expect(page.locator("#loading")).toBeHidden();
  await page.locator("#shop-open").click();
  const magnet = page.locator('[data-item="magnet"]');
  await expect(magnet.locator("button")).toBeDisabled();
  await expect(magnet).toContainText("Reach level 11");
  await expect(page.locator("#shop-wallet")).toHaveText("1,000,000");
  await page.keyboard.press("Escape");
  await page.evaluate(() => localStorage.setItem("altos-tower-v1", JSON.stringify({ save_version: 5, coins: 1000000, wallet: 1000000, post_campaign_meters: 99000, missions: { level: 61, values: [0, 0, 0], completed: [true, true, true], history: Array.from({ length: 60 }, (_, i) => i + 1) }, upgrades: { magnet: 5, feather: 0, wingsuit: 0, scarf: 0 } })));
  await page.reload(); await expect(page.locator("#loading")).toBeHidden(); await page.locator("#shop-open").click();
  await expect(magnet.locator("button")).toBeDisabled();
  await expect(magnet).toContainText("99.0/100 km");
  await page.keyboard.press("Escape");
  await page.evaluate(() => { const save=JSON.parse(localStorage.getItem("altos-tower-v1")!); save.post_campaign_meters=100000; localStorage.setItem("altos-tower-v1", JSON.stringify(save)); });
  await page.reload(); await expect(page.locator("#loading")).toBeHidden(); await page.locator("#shop-open").click();
  await expect(magnet.locator("button")).toBeEnabled(); await magnet.locator("button").click();
  await expect(page.locator("#shop-wallet")).toHaveText("920,000");
  await expect(magnet).toContainText("Maxed");
});


