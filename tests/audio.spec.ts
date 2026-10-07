import { test, expect } from '@playwright/test';

test('music and effects toggle independently, synchronize HUD, and persist on reload', async ({ page }) => {
  await page.goto('/'); await expect(page.locator('#loading')).toBeHidden();
  await page.locator('#sound').click();
  await expect(page.locator('#sound')).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('#sfx')).toHaveAttribute('aria-pressed','false');
  await page.locator('#zen').click();
  await expect(page.locator('#sound-play')).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('#sfx-play')).toHaveAttribute('aria-pressed','false');
  await page.locator('#sfx-play').click(); await page.locator('#sound-play').click();
  await expect(page.locator('#sfx-play')).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('#sound-play')).toHaveAttribute('aria-pressed','false');
  await page.reload(); await expect(page.locator('#loading')).toBeHidden();
  await expect(page.locator('#sfx')).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('#sound')).toHaveAttribute('aria-pressed','false');
});

test('music-only rejects effect voices and effects-only stops music scheduling while playing jumps', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(async () => {
    const {GameAudio}=await import('/src/audio.ts'); const audio=new GameAudio(); (window as any).__audioTest=audio;
    audio.setChannels(true,false);
    const button=document.createElement('button');button.id='activate-test-audio';button.textContent='Activate test audio';button.style.cssText='position:fixed;z-index:99999;top:0;left:0';button.onclick=()=>void audio.resumeGesture();document.body.append(button);
  });
  await page.locator('#activate-test-audio').click();
  await expect.poll(()=>page.evaluate(()=>(window as any).__audioTest.diagnostics.contextState)).toBe('running');
  const musicOnly=await page.evaluate(()=>{
    const a=(window as any).__audioTest;const s={time:1,distance:5,alive:true,grounded:true,rail:null,wing_on:false,chase:null,coins:0,boost:0,bank_time:0,banked_score:0,tricks:[],vy:0,zen:false};a.update(s,'play',1/60);const before=a.diagnostics.createdVoices;a.update({...s,time:2,grounded:false,vy:-20},'play',1/60);return {before,after:a.diagnostics.createdVoices,music:a.diagnostics.musicEnabled,effects:a.diagnostics.effectsEnabled};
  });
  expect(musicOnly.music).toBe(true);expect(musicOnly.effects).toBe(false);expect(musicOnly.after).toBe(musicOnly.before);
  await page.evaluate(()=>(window as any).__audioTest.setChannels(false,true));
  const ticks=await page.evaluate(()=>(window as any).__audioTest.diagnostics.musicTicks);await page.waitForTimeout(500);
  expect(await page.evaluate(()=>(window as any).__audioTest.diagnostics.musicTicks)).toBe(ticks);
  expect(await page.evaluate(()=>(window as any).__audioTest.diagnostics.scheduledTimer)).toBe(false);
  const effectsOnly=await page.evaluate(()=>{
    const a=(window as any).__audioTest;const s={time:3,distance:5,alive:true,grounded:true,rail:null,wing_on:false,chase:null,coins:0,boost:0,bank_time:0,banked_score:0,tricks:[],vy:0,zen:false};a.update(s,'play',1/60);const before=a.diagnostics.createdVoices;a.update({...s,time:4,grounded:false,vy:-20},'play',1/60);return {before,after:a.diagnostics.createdVoices,music:a.diagnostics.musicEnabled,effects:a.diagnostics.effectsEnabled};
  });
  expect(effectsOnly.music).toBe(false);expect(effectsOnly.effects).toBe(true);expect(effectsOnly.after).toBeGreaterThan(effectsOnly.before);
  await page.evaluate(()=>(window as any).__audioTest.dispose());
});

test('audio state edges are stable across duplicate renders, pause, death and run reset', async ({ page }) => {
  await page.goto('/');
  const result = await page.evaluate(async () => {
    const path = '/src/audio.ts';
    const { AudioEventTracker } = await import(path);
    const base = { time: 1, distance: 5, alive: true, grounded: true, rail: null, wing_on: false,
      chase: null, coins: 0, boost: 0, bank_time: 0, banked_score: 0, tricks: [], vy: 0 };
    const sequence = [
      base,
      { ...base, time: 2, grounded: false, vy: -15 },
      { ...base, time: 3, grounded: false, vy: -15, tricks: [{ name: 'Backflip', count: 1, points: 10 }] },
      { ...base, time: 4, grounded: false, rail: 7, tricks: [{ name: 'Backflip', count: 1, points: 10 }] },
      { ...base, time: 5, grounded: false, tricks: [{ name: 'Rock bounce', count: 1, points: 80 }] },
      { ...base, time: 6, bank_time: 2, banked_score: 80, boost: 4, coins: 10 },
      { ...base, time: 7, grounded: false, wing_on: true, chase: { x: 1 }, coins: 10, bank_time: 1, banked_score: 80, boost: 3 },
      { ...base, time: 8, alive: false, coins: 10 },
    ];
    const atRate = (duplicates: number) => {
      const tracker = new AudioEventTracker();
      const events: string[] = [];
      sequence.forEach(s => { for (let i = 0; i < duplicates; i++) events.push(...tracker.observe(s, s.alive ? 'play' : 'crash')); });
      return events;
    };
    const tracker = new AudioEventTracker();
    tracker.observe(base, 'play');
    const paused = tracker.observe({ ...base, coins: 100 }, 'pause');
    const resumed = tracker.observe({ ...base, coins: 100 }, 'play');
    const reset = tracker.observe({ ...base, time: 0, distance: 0 }, 'play');
    return { one: atRate(1), many: atRate(12), paused, resumed, reset };
  });
  expect(result.one).toEqual(['jump', 'flip', 'grind', 'bounce', 'coin', 'land', 'bank', 'boost', 'wing', 'chase', 'death']);
  expect(result.many).toEqual(result.one);
  expect(result.paused).toEqual([]); expect(result.resumed).toEqual([]); expect(result.reset).toEqual([]);
});

test('real Web Audio needs explicit activation, makes output, and releases bounded voices on mute/dispose', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(async () => {
    const path = '/src/audio.ts';
    const { GameAudio } = await import(path);
    const audio = new GameAudio();
    (window as any).__audioTest = audio;
    audio.setEnabled(true);
    const button = document.createElement('button'); button.id = 'activate-test-audio'; button.textContent = 'Test sound';
    button.style.cssText = 'position:fixed;z-index:99999;top:0;left:0';
    button.onclick = () => { void audio.resumeGesture(); };
    document.body.append(button);
  });
  expect(await page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('uninitialized');
  await page.locator('#activate-test-audio').click();
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('running');
  const rms = await page.evaluate(async () => {
    const audio = (window as any).__audioTest;
    const analyser = audio.context.createAnalyser(); analyser.fftSize = 2048;
    audio.master.connect(analyser);
    await new Promise(resolve => setTimeout(resolve, 350));
    const data = new Float32Array(analyser.fftSize); analyser.getFloatTimeDomainData(data);
    audio.master.disconnect(analyser); analyser.disconnect();
    return Math.sqrt(data.reduce((sum, x) => sum + x * x, 0) / data.length);
  });
  expect(rms).toBeGreaterThan(0.00001);
  await page.waitForTimeout(900);
  const running = await page.evaluate(() => (window as any).__audioTest.diagnostics);
  expect(running.musicTicks).toBeGreaterThanOrEqual(3);
  expect(running.createdVoices).toBeGreaterThan(5);
  expect(running.endedVoices).toBeGreaterThan(0);
  expect(running.peakVoices).toBeLessThanOrEqual(48);
  await page.evaluate(() => (window as any).__audioTest.setEnabled(false));
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.activeVoices)).toBe(0);
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('suspended');
  expect(await page.evaluate(() => (window as any).__audioTest.diagnostics.scheduledTimer)).toBe(false);
  await page.evaluate(() => (window as any).__audioTest.dispose());
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('closed');
  const disposed = await page.evaluate(() => (window as any).__audioTest.diagnostics);
  expect(disposed.persistentNodes).toBe(0);
  expect(disposed.createdVoices).toBe(disposed.endedVoices);
});

test('all distinct effects use the real audio graph; pause does not replay state and mute can resume', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(async () => {
    const path = '/src/audio.ts'; const { GameAudio } = await import(path);
    const audio = new GameAudio(); (window as any).__audioTest = audio;
    audio.setEnabled(true);
    const button = document.createElement('button'); button.id = 'activate-test-audio'; button.textContent = 'Test sound';
    button.style.cssText = 'position:fixed;z-index:99999;top:0;left:0';
    button.onclick = () => { audio.setEnabled(true); void audio.resumeGesture(); }; document.body.append(button);
  });
  await page.locator('#activate-test-audio').click();
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('running');
  const counts = await page.evaluate(() => {
    const audio = (window as any).__audioTest;
    const s = { time: 1, distance: 5, alive: true, grounded: true, rail: null, wing_on: false, chase: null,
      coins: 0, boost: 0, bank_time: 0, banked_score: 0, tricks: [], vy: 0, zen: false };
    audio.update(s, 'play', 1 / 60);
    audio.update({ ...s, time: 2, grounded: false, vy: -20 }, 'play', 1 / 60);
    audio.update({ ...s, time: 3, grounded: false, tricks: [{ name: 'Backflip', count: 1, points: 10 }] }, 'play', 1 / 60);
    audio.update({ ...s, time: 4, rail: 7, grounded: false }, 'play', 1 / 60);
    audio.update({ ...s, time: 5, grounded: false, tricks: [{ name: 'Rock bounce', count: 1, points: 80 }] }, 'play', 1 / 60);
    audio.update({ ...s, time: 6, coins: 10, boost: 3, bank_time: 2, banked_score: 80 }, 'play', 1 / 60);
    audio.update({ ...s, time: 7, coins: 10, grounded: false, wing_on: true, chase: { x: 1 } }, 'play', 1 / 60);
    audio.update({ ...s, time: 8, alive: false }, 'crash', 1 / 60);
    const before = JSON.stringify(audio.diagnostics.events);
    for (let i = 0; i < 120; i++) audio.update({ ...s, time: 9, coins: 999 }, 'pause', 1 / 120);
    return { events: audio.diagnostics.events, before, after: JSON.stringify(audio.diagnostics.events), peak: audio.diagnostics.peakVoices };
  });
  for (const event of ['jump', 'flip', 'grind', 'bounce', 'coin', 'land', 'bank', 'boost', 'wing', 'chase', 'death']) expect(counts.events[event]).toBe(1);
  expect(counts.after).toBe(counts.before); expect(counts.peak).toBeLessThanOrEqual(48);
  await page.evaluate(() => (window as any).__audioTest.setEnabled(false));
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('suspended');
  await page.locator('#activate-test-audio').click();
  await expect.poll(() => page.evaluate(() => (window as any).__audioTest.diagnostics.contextState)).toBe('running');
  await page.evaluate(() => (window as any).__audioTest.dispose());
});
