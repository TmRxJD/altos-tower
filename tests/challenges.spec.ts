import { test, expect } from '@playwright/test';

test('Challenges presents three scoped goals and a 60-level campaign', async ({page})=>{
  await page.goto('/'); await expect(page.locator('#loading')).toBeHidden();
  await page.locator('#challenges-open').click();
  await expect(page.locator('#challenge-panel')).toBeVisible();
  await expect(page.locator('.challenge-goal')).toHaveCount(3);
  await expect(page.locator('#challenge-heading')).toContainText('Level 1');
  await expect(page.locator('#challenge-unlock')).toContainText('Complete level 10');
  await expect(page.locator('#challenge-chapters button')).toHaveCount(6);
  await page.locator('[data-chapter="5"]').click(); await page.locator('[data-level="60"]').click();
  await expect(page.locator('#challenge-heading')).toContainText('Level 60');
  await expect(page.locator('.locked-challenges')).toContainText('Complete level 59');
  await page.locator('#challenge-current').click(); await expect(page.locator('.challenge-goal')).toHaveCount(3);
  const catalog=await page.evaluate(async()=>{const path='/src/wasm/engine.js';const {default:init,Game}=await import(path);await init();const g=new Game('{}');const data=JSON.parse(g.challenges());g.free();return data;});
  expect(catalog.levels).toHaveLength(60); expect(catalog.levels.every((l:any)=>l.goals.length===3)).toBe(true);
  await page.screenshot({path:'test-results/challenges-desktop.png'});
  await page.locator('#challenge-panel .close-panel').click(); await expect(page.locator('#challenges-open')).toBeFocused();
});

test('Challenge inspection pauses a run and returns to Resume',async({page})=>{
  await page.goto('/');await expect(page.locator('#loading')).toBeHidden();await page.locator('#start').click();
  await page.locator('#run-challenges').click();await expect(page.locator('#challenge-panel')).toBeVisible();
  const x=await page.locator('#scene').getAttribute('data-world-x');await page.waitForTimeout(250);
  expect(await page.locator('#scene').getAttribute('data-world-x')).toBe(x);
  await page.keyboard.press('Escape');await expect(page.locator('#dialog-title')).toHaveText('Paused');
  await expect(page.locator('#resume')).toBeFocused();await page.locator('#resume').click();await expect(page.locator('#overlay')).toBeHidden();
});

test('Splash and challenge controls fit portrait and landscape',async({page})=>{
  for(const [width,height] of [[360,640],[844,390],[1440,900]]){
    await page.setViewportSize({width,height});await page.goto('/');await expect(page.locator('#loading')).toBeHidden();
    const fit=await page.locator('.launch-dock').evaluate(el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,buttons:[...el.querySelectorAll('button')].map(b=>b.getBoundingClientRect().height),overflow:document.documentElement.scrollWidth>innerWidth};});
    expect(fit.y).toBeGreaterThanOrEqual(0);expect(fit.x).toBeGreaterThanOrEqual(0);expect(fit.right).toBeLessThanOrEqual(width);expect(fit.bottom).toBeLessThanOrEqual(height);expect(fit.overflow).toBe(false);expect(fit.buttons.every(h=>h>=44)).toBe(true);
    await page.screenshot({path:`test-results/splash-${width}.png`});
    await page.locator('#challenges-open').click();await expect(page.locator('#challenge-panel')).toBeVisible();
    await page.screenshot({path:`test-results/challenges-${width}.png`});
    await page.locator('#challenge-panel .close-panel').click();
  }
});

test('Old easy progression is replaced while earned funds and upgrades survive',async({page})=>{
  await page.addInitScript(()=>localStorage.setItem('altos-tower-v1',JSON.stringify({save_version:4,meters:999999,coins:10000,wallet:9000,runs:20,selected:5,upgrades:{magnet:2,feather:1,wingsuit:1,scarf:0},flips:1000})));
  await page.goto('/');await expect(page.locator('#loading')).toBeHidden();
  await expect(page.locator('#selected')).toContainText('Alto');await expect(page.locator('#wallet-total')).toHaveText('9,000');
  await page.locator('#challenges-open').click();await expect(page.locator('#challenge-heading')).toContainText('Level 1');await expect(page.locator('#challenge-migration')).toBeVisible();
  await page.locator('#challenge-panel .close-panel').click();await page.locator('#shop-open').click();
  await expect(page.locator('[data-item="magnet"] .upgrade-pips .filled')).toHaveCount(2);
});

test('Felipe shield encloses his ears and head',async({page})=>{
  await page.goto('/');await expect(page.locator('#loading')).toBeHidden();
  const enclosed=await page.evaluate(async()=>{
    const path='/src/wasm/engine.js', renderer='/src/scene.ts'; const {default:init,Game}=await import(path);await init();const {Scene}=await import(renderer);
    const g=new Game('{}');g.start_seeded(true,2);const s=JSON.parse(g.snapshot());g.free();s.x=6000;s.y=300;s.rider=5;s.angle=0;s.shield=true;s.features=[];s.chase=null;s.camp=null;s.gaps=[];
    const canvas=document.createElement('canvas');canvas.id='shield-fixture';canvas.style.cssText='position:fixed;inset:0;width:100vw;height:100vh;z-index:1000';document.body.append(canvas);const scene=new Scene(canvas,{});
    await new Promise<void>(r=>requestAnimationFrame(()=>requestAnimationFrame(()=>r())));
    const ctx=canvas.getContext('2d')!,ellipse=ctx.ellipse.bind(ctx);let field:number[]|null=null;
    ctx.ellipse=(...args:Parameters<typeof ctx.ellipse>)=>{if(args[2]>35&&args[3]>35)field=args.slice(0,4);ellipse(...args);};scene.render(s,'play',0);
    if(!field)return false;const [x,y,rx,ry]=field as number[];return [[16,-29],[22,-29],[25,-22]].every(([hx,hy])=>((hx-x)/rx)**2+((hy-y)/ry)**2<.9);
  });expect(enclosed).toBe(true);await page.screenshot({path:'test-results/felipe-shield.png'});
});
