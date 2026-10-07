import { test, expect } from '@playwright/test';

test('fullscreen fallback keeps phone play controls reachable', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 568 });
  await page.addInitScript(() => Object.defineProperty(document, 'fullscreenEnabled', { get: () => false }));
  await page.goto('/');
  await expect(page.locator('#loading')).toBeHidden();
  await page.locator('#topbar [data-fullscreen]').click();
  await expect(page.locator('html')).toHaveClass(/viewport-play/);
  await expect(page.locator('.fullscreen-status')).toContainText('full browser viewport');
  await page.locator('#start').click();
  await expect(page.locator('#jump')).toBeInViewport();
  await expect(page.locator('#pause')).toBeInViewport();
  await page.setViewportSize({ width: 568, height: 320 });
  await expect(page.locator('#jump')).toBeInViewport();
  await expect(page.locator('#pause')).toBeInViewport();
  await page.locator('#hud [data-fullscreen]').click();
  await expect(page.locator('html')).not.toHaveClass(/viewport-play/);
});
