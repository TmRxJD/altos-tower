import { setupFullscreen } from './fullscreen';
import init, { Game } from "./wasm/engine";
import { Scene } from "./scene";
import { GameAudio } from "./audio";
import type { State, Mode, ShopItem, Challenges } from "./types";
import "./style.css";
const app = document.querySelector<HTMLDivElement>("#app")!;
app.innerHTML = `<canvas id="scene" aria-label="Downhill enemy rider. Space or tap to jump; hold to flip. W toggles wingsuit. Enemy traits are passive."></canvas><div class="grain"></div>
<header id="topbar" class="topbar"><span class="brand"><i class="sigil"></i>ALTO’S TOWER</span><div class="audio-controls"><button data-fullscreen class="audio-toggle" aria-label="Enter fullscreen">⛶</button><button id="sound" class="audio-toggle" aria-label="Enable music" aria-pressed="false">Music</button><button id="sfx" class="audio-toggle" aria-label="Enable sound effects" aria-pressed="false">SFX</button></div></header>
<main id="menu" class="menu"><h1 class="title">Alto’s Tower</h1><div class="launch-dock"><div class="actions"><button id="start" class="primary">Play</button><button id="zen" class="quiet">Zen</button></div><button id="roster-open" class="dock-row"><span id="selected-icon" aria-hidden="true"></span><span id="selected">Roster · Alto</span><span aria-hidden="true">›</span></button><button id="challenges-open" class="dock-row"><span>Challenges</span><span id="menu-missions"></span><span aria-hidden="true">›</span></button><div class="dock-tools"><button id="shop-open">Shop · <span id="wallet-total">0</span> coins</button><button id="guide-open">Controls</button></div><button id="skin" class="appearance-button">Switch to Tower</button></div></main>
<section id="roster-panel" class="panel" role="dialog" aria-modal="true" aria-label="Character roster" hidden><div class="panel-head"><h2>Characters</h2><button class="close-panel">Back</button></div><div id="roster" class="roster"></div></section>
<section id="guide-panel" class="panel" role="dialog" aria-modal="true" aria-label="Controls and hazards" hidden><div class="panel-head"><h2>Controls</h2><button class="close-panel">Back</button></div><div class="control-keys"><span><kbd>Space</kbd><i class="touch-key">Tap · </i>Jump · hold to flip · release to recover</span><span><kbd>W</kbd><i class="touch-key">Tap · </i>Wingsuit · hold to loop · release to dive</span><span><kbd>P</kbd><i class="touch-key">Tap · </i>Pause</span></div><div class="legend"><span class="safe">✦ Powerup</span><span class="safe">━ Grind</span><span class="danger">× Danger</span><span class="energy">● Coins</span></div><div id="guide" class="guide-grid"></div></section>
<div id="hud" class="hud" hidden><div class="stats"><strong id="distance">0 m</strong><span id="score">0</span><span class="energy">◇ <span id="coins">0</span></span></div><div class="hud-actions"><button id="run-challenges" aria-label="Open challenges">Goals <span id="goal-count">0/3</span></button><div class="audio-controls audio-play"><button id="sound-play" class="audio-toggle" aria-label="Enable music" aria-pressed="false">Music</button><button id="sfx-play" class="audio-toggle" aria-label="Enable sound effects" aria-pressed="false">SFX</button></div><button data-fullscreen class="icon-button" aria-label="Enter fullscreen">⛶</button><button id="pause" class="icon-button" aria-label="Pause" title="Pause (P)">Ⅱ</button></div></div><div id="buffs" class="buffs" hidden></div><div id="run-info" class="run-info" hidden></div><div id="notice" class="notice" aria-live="polite"></div><div id="combo" class="combo" hidden></div>
<div id="touch-controls" class="touch-controls" hidden><button id="jump" aria-label="Jump, hold to flip" class="jump-button">↑<small>Jump</small></button><div class="ability-controls"><button id="wing" aria-label="Wingsuit" title="Wingsuit (W)">⋈<small>Wingsuit</small><span class="wing-meter"></span><span class="wing-status"></span></button></div><span class="desktop-hint"><kbd>Space</kbd> jump · hold flip</span></div>
<div id="overlay" class="overlay" hidden><section class="dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title"><h2 id="dialog-title">Paused</h2><p id="dialog-detail"></p><div id="result"></div><div class="actions"><button id="resume" class="primary">Resume</button><button id="retry">Retry</button><button id="home">Menu</button><button id="pause-controls">Controls</button></div></section></div>
<div id="loading" class="loading"><span class="sigil"></span></div><div id="toast" class="toast" hidden></div>`;
app.insertAdjacentHTML("beforeend", `<dialog id="shop-panel" class="panel shop-panel" aria-labelledby="shop-title"><div class="panel-head"><h2 id="shop-title">Shop</h2><div class="shop-balance"><span class="coin-symbol" aria-hidden="true"></span><strong id="shop-wallet">0</strong><span>coins</span></div><button id="shop-close">Back</button></div><div id="shop-items"></div><p id="shop-feedback" class="shop-feedback" role="status" aria-live="polite"></p></dialog>`);
const el = (id: string) => document.getElementById(id)!;
setupFullscreen();
const shopPanel = el("shop-panel") as HTMLDialogElement;
const canvas = el("scene") as HTMLCanvasElement;
const art: Record<string, HTMLImageElement> = {};
const path = (n: string) =>
  `${import.meta.env.BASE_URL}art/${n.toLowerCase().replaceAll(" ", "-")}.webp`;
const esc = (s: string) =>
  s.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
let game: Game,
  s: State,
  scene: Scene,
  mode: Mode = "menu",
  last = 0,
  acc = 0,
  saveClock = 0,
  uiClock = 0,
  storageOK = true,
  everJumped = false;
let resumeMode: "play" | "crash" = "play";
let titleSeed: number | null = null;
let titleWorld: State | null = null;
let music = false, effects = false;
try { const audioPrefs=JSON.parse(localStorage.getItem("altos-tower-audio")??"{}"); music=audioPrefs.music===true; effects=audioPrefs.effects===true; } catch { /* Defaults remain silent. */ }
const gameAudio = new GameAudio();
let panelReturn: HTMLElement | null = null;
let skin: "classic" | "tower" = "classic";
const classicNames = ["Alto", "Maya", "Paz", "Izel", "Tupa", "Felipe"];
const riderIcon = (index: number) => `<span class="rider-portrait ${skin}" aria-hidden="true" style="--portrait-x:${index%3*50}%;--portrait-y:${Math.floor(index/3)*100}%"></span>`;
const riderName = (index: number) => skin === "classic" ? classicNames[index] : s.riders[index].name;

function seed() {
  return crypto.getRandomValues(new Uint32Array(1))[0] || 1;
}
function toast(text: string) {
  el("toast").textContent = text;
  el("toast").hidden = false;
  window.setTimeout(() => (el("toast").hidden = true), 2500);
}
function save() {
  try {
    localStorage.setItem("altos-tower-v1", game.save());
  } catch {
    if (storageOK) {
      storageOK = false;
      toast("Progress cannot be saved in this browser.");
    }
  }
}
function missions() {
  return `<span>Level ${s.level}/60 · ${s.goals.filter(g=>g.complete).length}/3</span>`;
}

function snapshot() {
  s = JSON.parse(game.snapshot());
  if (mode === "menu" && titleWorld) {
    s = { ...titleWorld, alive: false, rider: s.rider, riders: s.riders, progress: s.progress, goals: s.goals, level: s.level };
  }
  canvas.dataset.runSeed = String(s.seed ?? 0);
  canvas.dataset.worldX = String(s.x);
  canvas.dataset.poseAngle = String(s.angle);
  canvas.dataset.wingCharge = String(s.wing);
  canvas.dataset.motion = s.rail != null ? "grind" : s.grounded ? "ground" : s.wing_on ? "flight" : "air";
  canvas.dataset.speed = String(s.speed);
  canvas.dataset.boost = String(s.boost ?? 0);
  canvas.dataset.ruleset = String(s.ruleset ?? 2);
}
function menu() {
  if (mode !== "menu" || !titleWorld) {
    titleSeed = seed();
    const preview = new Game(game.save());
    preview.start_seeded(false, titleSeed);
    titleWorld = JSON.parse(preview.snapshot());
    preview.free();
  }
  scene?.presentation.clear();
  el("menu").inert = false;
  for (const animation of el("menu").getAnimations()) animation.cancel();
  mode = "menu";
  game.release();
  save();
  snapshot();
  el("menu").hidden = false;
  el("topbar").hidden = false;
  el("topbar").inert = false;
  for (const id of [
    "hud",
    "buffs",
    "run-info",
    "touch-controls",
    "combo",
    "overlay",
    "roster-panel",
    "challenge-panel",
    "guide-panel",
  ])
    el(id).hidden = true;
  el("notice").textContent = "";
  el("menu-missions").innerHTML = missions();
  el("wallet-total").textContent = s.progress.wallet.toLocaleString();
  el("selected").textContent = `Roster · ${riderName(s.rider)}`;
  el("selected-icon").innerHTML = riderIcon(s.rider);

}
function start(zen: boolean) {
  if(music||effects)void gameAudio.resumeGesture();
  scene.beginEntrance(!el("menu").hidden);
  const seeded = game as Game & {
    start_seeded?: (zen: boolean, seed: number) => void;
  };
  if (seeded.start_seeded) seeded.start_seeded(zen, titleSeed ?? seed());
  else game.start(zen);
  titleSeed = null;
  mode = "play";
  snapshot();
  acc = 0;
  everJumped = false;
  const departingMenu = el("menu");
  if (!departingMenu.hidden) {
    departingMenu.inert = true;
    const fade = departingMenu.animate([{ opacity: 1, transform: "translateY(0)" }, { opacity: 0, transform: "translateY(-12px)" }], { duration: scene.presentation.reduced ? 120 : 420, easing: "ease-out" });
    fade.onfinish = () => { if (mode !== "menu") departingMenu.hidden = true; };
  }
  for (const id of ["topbar", "overlay", "roster-panel", "guide-panel", "challenge-panel"])
    el(id).hidden = true;
  (el("jump") as HTMLButtonElement).disabled = false;
  for (const id of ["hud", "buffs", "run-info", "touch-controls", "combo"]) {
    el(id).hidden = false;
    el(id).inert = false;
  }
  save();
  canvas.focus();
  updateHUD();
}
function resume() {
  mode = resumeMode;
  acc = 0;
  el("overlay").hidden = true;
  el("hud").inert = false;
  el("touch-controls").inert = mode === "crash";
  canvas.focus();
}
function shortNotice(n: string) {
  if (/Rail grind|Backflip|Night flight|Clean landing/.test(n)) return "";
  if (/Tap|Hold in/.test(n)) return everJumped ? "" : "Tap · hold to flip";
  if (/landing.*\+/.test(n)) return n.match(/\+\d+/)?.[0] ?? "Landed";
  if (/Unfinished|upside|orientation/i.test(n)) return "Bad landing";
  if (/Enemy collision/.test(n)) return "Collision";
  if (/Lost|chasm/.test(n)) return "Missed the gap";
  if (/Breathe|Zen|recover/i.test(n)) return "Zen · recovered";
  return n.split(" · ")[0].replace("Mission set complete", "Level up");
}
function dialog(dead = false) {
  if (!dead && mode !== "pause") resumeMode = mode === "crash" ? "crash" : "play";
  game.release();
  mode = dead ? "result" : "pause";
  save();
  el("hud").inert = true;
  el("touch-controls").inert = true;
  el("overlay").hidden = false;
  el("resume").hidden = dead;
  el("retry").classList.toggle("primary", dead);
  el("dialog-title").textContent = dead ? "Run over" : "Paused";
  el("dialog-detail").textContent = dead ? shortNotice(s.notice) : "";
  el("result").innerHTML = dead
    ? `<div class="result-stats"><strong>${s.distance.toLocaleString()}<small>metres</small></strong><strong>${s.score.toLocaleString()}<small>points</small></strong><strong>${s.coins}<small>coins</small></strong></div><button id="result-challenges" class="result-challenges">Challenges · ${missions()} ›</button>`
    : "";
  if (dead) el("result-challenges").onclick = openChallenges;
  (dead ? el("retry") : el("resume")).focus();
}
function updateHUD() {
  el("distance").textContent = `${s.distance.toLocaleString()} m`;
  el("score").textContent = `${s.score.toLocaleString()} pts`;
  el("coins").textContent = String(s.coins);
  el("goal-count").textContent = `${s.goals.filter(g=>g.complete).length}/3`;
  const boost = s.boost ?? 0;
  const remaining = Math.max(0, Math.min(1, boost / (s.boost_max ?? 2)));
  const range = (seconds: number) => Math.ceil(seconds * Math.abs(s.vx ?? s.speed) / 40);
  const buff = (label: string, seconds: number, fraction: number) => `<span class="boost-status buff-range" style="--remaining:${fraction}" title="Estimated remaining range at current speed"><strong>${label}</strong> ≈${range(seconds)} m<i></i></span>`;
  const buffs = (boost > 0 ? buff("BOOST", boost, remaining) : "")
    + (s.magnet ? buff("◎", s.magnet, s.magnet / (5 + s.progress.upgrades.magnet * 2.5)) : "")
    + (s.feather ? buff("◌", s.feather, s.feather / (5 + s.progress.upgrades.feather * 2.5)) : "")
    + (s.shield ? `<span class="shield-status">◇ Shield</span>` : "");
  if(el("buffs").innerHTML !== buffs) el("buffs").innerHTML = buffs;
  el("run-info").textContent = s.zen ? "ZEN · PRACTICE" : "";
  el("run-info").setAttribute("aria-label", s.chase ? `Elder ${Math.max(0, Math.round((s.x-s.chase.x)/40))} metres behind` : "Run information");
  el("notice").textContent = shortNotice(s.notice);
  const combo = el("combo"), banked = !s.combo && (s.bank_time ?? 0) > 0;
  const tricks = (s.combo ? s.tricks : s.banked_tricks) ?? [];
  const markup = tricks.slice(-4).map(t => `<div class="trick-line">${esc(t.name)}${s.rail != null && /grind/i.test(t.name) && s.grind_distance_m ? ` ${Math.floor(s.grind_distance_m)} m` : t.count > 1 ? ` ×${t.count}` : ""}</div>`).join("")
    + (s.combo ? `<strong class="combo-total">${s.combo_points} × ${s.combo}</strong>` : banked ? `<strong class="combo-total">+${s.banked_score}</strong>` : "");
  if (combo.innerHTML !== markup) combo.innerHTML = markup;
  combo.hidden = !s.combo && !banked;
  combo.classList.toggle("banked", banked);
  const wing = el("wing") as HTMLButtonElement;
  const effectiveWing = (s.scarf_length ?? s.wing) / 6;
  wing.hidden = false;
  wing.disabled = mode === "crash" || (!s.wing_on && (!s.wing_ready || s.grounded));
  wing.style.setProperty("--charge", String(Math.min(1, effectiveWing)));
  wing.querySelector(".wing-status")!.textContent =
    s.wing_ready ? "Ready" : `${Math.round(effectiveWing * 100)}%`;
  wing.classList.toggle("active", s.wing_on);
  wing.title =
    !s.wing_ready
      ? "Land combos to extend your scarf; long grinds refresh it"
      : "Wingsuit · hold to soar and loop, release to dive";
}
function openPanel(id: string) {
  panelReturn = document.activeElement as HTMLElement;
  el("menu").hidden = true;
  el("topbar").inert = true;
  el("overlay").hidden = true;
  el(id).hidden = false;
  el(id).querySelector<HTMLButtonElement>("button")!.focus();
}
function closePanels() {
  el("roster-panel").hidden = true;
  el("guide-panel").hidden = true;
  el("challenge-panel").hidden = true;
  el("topbar").inert = false;
  if (mode === "menu") el("menu").hidden = false;
  else if (mode === "pause" || mode === "result") el("overlay").hidden = false;
  save();
  if (mode === "pause") el("resume").focus();
  else if (mode === "result") el("result-challenges").focus();
  else (panelReturn?.isConnected ? panelReturn : el("start"))?.focus();
}
function roster() {
  el("roster-panel").setAttribute("aria-label", skin === "classic" ? "Character roster" : "Enemy roster");
  el("roster-panel").querySelector("h2")!.textContent = skin === "classic" ? "Characters" : "Enemies";
  el("roster").innerHTML = s.riders
    .map(
      (r, i) =>
        `<button data-rider="${i}" class="rider ${s.rider === i ? "selected" : ""}" aria-pressed="${s.rider === i}" title="${r.level > s.level ? `Complete level ${r.level-1} to unlock` : r.ability}" ${r.level > s.level ? "disabled" : ""}>${riderIcon(i)}<strong>${riderName(i)}</strong><small>${r.level > s.level ? `Complete level ${r.level-1}` : s.rider === i ? "Selected" : r.ability}</small><p>${r.description}</p></button>`,
    )
    .join("");
  openPanel("roster-panel");
}
function renderShop() {
  snapshot();
  const items: ShopItem[] = JSON.parse(game.shop());
  const glyphs: Record<string, string> = { magnet: "◎", feather: "◌", wingsuit: "⋈", scarf: "∿", helmet: "◇", rescue: "⌁" };
  el("shop-wallet").textContent = s.progress.wallet.toLocaleString();
  el("wallet-total").textContent = s.progress.wallet.toLocaleString();
  el("shop-items").innerHTML = ["Upgrades", "Safety"].map(group => `<section class="shop-group" aria-label="${group}"><div class="shop-group-title"><h3>${group}</h3>${group === "Safety" ? "<span>Automatic · consumed when used</span>" : ""}</div><ul class="shop-list">${items.filter(i => i.group === group).map(i => {
    const capped = i.price === null;
    const missing = i.price === null || i.locked ? 0 : Math.max(0, i.price - s.progress.wallet);
    return `<li class="shop-row" data-item="${i.id}"><span class="shop-glyph" aria-hidden="true">${glyphs[i.id]}</span><div class="shop-description"><strong>${esc(i.name)}</strong><span>${esc(i.effect)}</span>${group === "Upgrades" ? `<div class="upgrade-pips" aria-label="Level ${i.level} of ${i.max}">${Array.from({length: i.max}, (_, n) => `<i class="${n < i.level ? "filled" : ""}"></i>`).join("")}</div>` : `<small>${i.level} owned · carry up to ${i.max}</small>`}</div><div class="shop-purchase"><button data-buy="${i.id}" aria-label="${group === "Upgrades" ? "Upgrade" : "Buy"} ${esc(i.name)}" ${!i.affordable ? "disabled" : ""}>${i.locked ? "Locked" : capped ? group === "Upgrades" ? "Maxed" : "Full" : `${group === "Upgrades" ? "Upgrade" : "Buy"} <b>${i.price!.toLocaleString()}</b>`}</button>${i.locked && i.requirement ? `<small class="shop-requirement">${esc(i.requirement)}</small>` : missing ? `<small>Need ${missing.toLocaleString()} more</small>` : ""}</div></li>`;
  }).join("")}</ul></section>`).join("");
}
function openShop() {
  if (mode !== "menu") return;
  renderShop();
  el("shop-feedback").textContent = "";
  shopPanel.showModal();
}
const guide = [
  ["Ranged", "Magnet draws nearby coins."],
  ["Protector", "Feather lifts you over rocks; it never launches you."],
  ["Tank", "Shield saves one rock hit or bad landing."],
];
el("guide").innerHTML = guide
  .map(
    ([title, text]) =>
      `<div class="guide-card"><img src="${path(title)}" alt=""><h3>${({Ranged:"Magnet",Protector:"Feather",Tank:"Shield"} as Record<string,string>)[title]}</h3><p>${text}</p></div>`,
  )
  .join("") + `<div class="guide-card"><h3>Wallride</h3><p>Hold Jump inside a wall face; release to kick away.</p></div><div class="guide-card"><h3>Balloons</h3><p>Land on the canopy to bounce and keep your combo.</p></div><div class="guide-card"><h3>Wingsuit</h3><p>Land tricks to charge · ready at 80%. Skim the ground for proximity points.</p></div>`;
el("skin").onclick = () => {
  skin = skin === "classic" ? "tower" : "classic";
  scene.setSkin(skin);
  el("skin").textContent = skin === "classic" ? "Switch to Tower" : "Switch to Classic";
  menu();
};
el("start").onclick = () => start(false);
el("zen").onclick = () => start(true);
el("roster-open").onclick = roster;
el("shop-open").onclick = openShop;
el("shop-close").onclick = () => shopPanel.close();
shopPanel.addEventListener("close", () => { snapshot(); save(); el("wallet-total").textContent = s.progress.wallet.toLocaleString(); });
el("shop-items").onclick = (e) => {
  const button = (e.target as Element).closest<HTMLButtonElement>("[data-buy]");
  if (!button || button.disabled) return;
  const id = button.dataset.buy!, items: ShopItem[] = JSON.parse(game.shop());
  const item = items.find(i => i.id === id);
  if (!item || !game.purchase(id)) return;
  save(); renderShop();
  el("shop-feedback").textContent = `${item.name} ${item.group === "Upgrades" ? "upgraded" : "added"}`;
  const next = shopPanel.querySelector<HTMLButtonElement>(`[data-buy="${id}"]`);
  (next && !next.disabled ? next : el("shop-close")).focus({ preventScroll: true });
};
el("guide-open").onclick = () => openPanel("guide-panel");
el("pause-controls").onclick = () => openPanel("guide-panel");
document
  .querySelectorAll<HTMLButtonElement>(".close-panel")
  .forEach((b) => (b.onclick = closePanels));
el("roster").onclick = (e) => {
  const b = (e.target as Element).closest<HTMLButtonElement>("[data-rider]");
  if (b && game.select(Number(b.dataset.rider))) {
    snapshot();
    roster();
  }
};
el("pause").onclick = () => {
  if (mode === "play" || mode === "crash") dialog();
};
el("resume").onclick = resume;
el("retry").onclick = () => start(s.zen);
el("home").onclick = () => {
  game = new Game(game.save());
  menu();
};
el("wing").onclick = () => {
  if (mode === "play") {
    game.wingsuit();
    canvas.focus();
  }
};
function jump() {
  if (mode === "crash") {
    if (scene.presentation.time >= 0.3) dialog(true);
    return;
  }
  if (mode === "play") {
    if (scene.presentation.phase === "intro") scene.presentation.clear();
    game.press();
    everJumped = true;
  }
}
window.addEventListener("keydown", (e) => {
  if (shopPanel.open) return;
  if (e.key === "Tab") {
    const p = ["roster-panel", "guide-panel", "challenge-panel", "overlay"]
      .map(el)
      .find((p) => !p.hidden);
    if (p) {
      const b = [
        ...p.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
      ].filter((b) => !b.hidden);
      if (e.shiftKey && document.activeElement === b[0]) {
        e.preventDefault();
        b[b.length - 1].focus();
      } else if (!e.shiftKey && document.activeElement === b[b.length - 1]) {
        e.preventDefault();
        b[0].focus();
      }
    }
    return;
  }
  if (!["Space", "KeyW", "KeyP", "Escape"].includes(e.code) || e.repeat) return;
  if (e.code === "Space" && (e.target as Element).closest("button")) return;
  e.preventDefault();
  if (e.code === "Space") jump();
  else if (e.code === "KeyW" && mode === "play") game.wingsuit();
  else if (e.code === "KeyP" || e.code === "Escape") {
    if (!el("roster-panel").hidden || !el("guide-panel").hidden || !el("challenge-panel").hidden) closePanels();
    else if (mode === "play" || mode === "crash") dialog();
    else if (mode === "pause") resume();
  }
});
window.addEventListener("keyup", (e) => {
  if (e.code === "Space") game?.release();
});
for (const surface of [canvas, el("jump")]) {
  surface.addEventListener("pointerdown", (e) => {
    const p = e as PointerEvent;
    if ((mode !== "play" && mode !== "crash") || p.button !== 0) return;
    e.preventDefault();
    surface.setPointerCapture(p.pointerId);
    jump();
  });
  for (const t of ["pointerup", "pointercancel", "lostpointercapture"])
    surface.addEventListener(t, () => game?.release());
}
window.addEventListener("blur", () => {
  if (mode === "play" || mode === "crash") dialog();
});
document.addEventListener("visibilitychange", () => {
  if (document.hidden && (mode === "play" || mode === "crash")) dialog();
});
window.addEventListener("pagehide", () => {
  if (game) save();
});
function syncAudio() {
  for(const [ids,on,label] of [[ ["sound","sound-play"],music,"music" ],[ ["sfx","sfx-play"],effects,"sound effects" ]] as const) {
    for(const id of ids) { el(id).setAttribute("aria-pressed",String(on)); el(id).setAttribute("aria-label",`${on?"Mute":"Enable"} ${label}`); }
  }
  gameAudio.setChannels(music,effects);
}
function toggleAudio(channel: "music"|"effects") {
  if(channel==="music")music=!music;else effects=!effects;
  syncAudio();
  try { localStorage.setItem("altos-tower-audio",JSON.stringify({music,effects})); } catch { /* Session preference still applies. */ }
  if(music||effects)void gameAudio.resumeGesture();
}
el("sound").onclick = ()=>toggleAudio("music");
el("sfx").onclick = ()=>toggleAudio("effects");
el("sound-play").onclick = ()=>toggleAudio("music");
el("sfx-play").onclick = ()=>toggleAudio("effects");
syncAudio();
window.addEventListener("pagehide",()=>gameAudio.dispose());
function frame(now: number) {
  const dt = Math.min((now - last) / 1000 || 0, 0.05);
  last = now;
  if (mode === "play" || mode === "crash") {
    if (scene.presentation.advance(dt)) {
      if (scene.presentation.phase === "crash") dialog(true);
      else scene.presentation.clear();
    }
  }
  canvas.dataset.presentation = scene.presentation.phase ?? "ride";
  canvas.dataset.presentationTime = String(scene.presentation.time);
  app.dataset.presentation = scene.presentation.phase ?? "ride";
  app.style.setProperty("--entrance-opacity", String(scene.presentation.phase === "intro" ? Math.min(1, scene.presentation.progress * 3) : 1));
  if (mode === "play") {
    acc += dt * scene.presentation.simulationRate;
    while (acc >= 1 / 120) {
      game.tick(1 / 120);
      acc -= 1 / 120;
    }
    const priorTricks = JSON.stringify([s.combo, s.tricks, s.banked_score, (s.boost ?? 0) > 0]);
    snapshot();
    if (!s.alive) {
      mode = "crash";
      game.release();
      save();
      scene.beginCrash(s);
      el("touch-controls").inert = true;
      (el("jump") as HTMLButtonElement).disabled = true;
      (el("wing") as HTMLButtonElement).disabled = true;
      canvas.focus();
    }
    saveClock += dt;
    uiClock += dt;
    if (saveClock > 2) {
      save();
      saveClock = 0;
    }
    if (uiClock > 0.06 || priorTricks !== JSON.stringify([s.combo, s.tricks, s.banked_score, (s.boost ?? 0) > 0])) {
      updateHUD();
      uiClock = 0;
    }
  }
  let visual = s;
  if (mode === "play" && s.previous) {
    const a = Math.min(1, acc * 120),
      prev = s.previous;
    const delta = Math.atan2(
      Math.sin(s.angle - prev.angle),
      Math.cos(s.angle - prev.angle),
    );
    const lag =
      (s.terrain_time ?? s.time) -
      (prev.terrain_time +
        ((s.terrain_time ?? s.time) - prev.terrain_time) * a);
    visual = {
      ...s,
      x: prev.x + (s.x - prev.x) * a,
      y: prev.y + (s.y - prev.y) * a,
      angle: prev.angle + delta * a,
      time: prev.time + (s.time - prev.time) * a,
      chase: s.chase ? { ...s.chase,
        x: s.chase.previous_x + (s.chase.x - s.chase.previous_x) * a,
        y: s.chase.previous_y + (s.chase.y - s.chase.previous_y) * a,
      } : null,
      terrain: s.terrain.map((p) => ({ ...p, y: p.y - (p.vy ?? 0) * lag })),
    };
  }
  scene.render(visual, mode, dt);
  gameAudio.update(s, mode, dt);
  requestAnimationFrame(frame);
}
async function boot() {
  try {
    await init();
    let saved = "{}";
    try {
      saved = localStorage.getItem("altos-tower-v1") ?? "{}";
    } catch {
      storageOK = false;
    }
    game = new Game(saved);
    snapshot();
    await Promise.all(
      [
        ...s.riders.map((r) => r.name),
        "Boss",
        "Ray",
        "Scatter",
        ...guide.map((g) => g[0]),
        "Player",
      ]
        .filter((n, i, a) => a.indexOf(n) === i)
        .map(async (n) => {
          const img = new Image();
          img.src = path(n);
          await img.decode();
          art[n] = img;
        }),
    );
    scene = new Scene(canvas, art);
    canvas.tabIndex = -1;
    el("loading").hidden = true;
    menu();
    requestAnimationFrame(frame);
  } catch (error) {
    el("loading").innerHTML = `<p>${esc(String(error))}</p>`;
    console.error(error);
  }
}
void boot();
app.insertAdjacentHTML("beforeend", `<section id="challenge-panel" class="panel challenge-panel" role="dialog" aria-modal="true" aria-labelledby="challenge-title" hidden><div class="panel-head"><div><span class="eyebrow">THE DESCENT</span><h2 id="challenge-title">Challenges</h2></div><button class="close-panel">Back</button></div><div class="challenge-content"><div id="challenge-heading"></div><div id="challenge-goals" class="challenge-goals"></div><p id="challenge-unlock" class="challenge-unlock"></p><p id="challenge-migration" class="challenge-migration" hidden>New challenges start at level 1. Your coins and shop upgrades are kept.</p><div class="campaign-heading"><h3>Campaign</h3><button id="challenge-current">Current level</button></div><div id="challenge-chapters" class="challenge-chapters" aria-label="Campaign chapters"></div><div id="challenge-levels" class="challenge-levels" aria-label="Levels in this chapter"></div></div></section>`);
let challengeLevel = 1;
function challengeData(): Challenges {
  const enhanced = game as Game & { challenges?: () => string };
  return enhanced.challenges ? JSON.parse(enhanced.challenges()) : { level:s.level,total:60,complete:false,current_goals:s.goals,levels:[],history:[],migrated:false };
}
function renderChallenges() {
  const data = challengeData(), locked = challengeLevel > data.level, completed = challengeLevel < data.level || data.complete;
  const selected = data.levels.find(l=>l.level===challengeLevel);
  const goals = challengeLevel===data.level ? data.current_goals : selected?.goals ?? [];
  el("challenge-heading").innerHTML = `<span class="eyebrow">${completed ? "COMPLETED" : locked ? "LOCKED" : "CURRENT LEVEL"}</span><h3>Level ${challengeLevel} <small>of 60</small></h3>`;
  el("challenge-goals").innerHTML = locked ? `<p class="locked-challenges">Complete level ${challengeLevel-1} to unlock these challenges.</p>` : goals.map(g=> {
    const done=completed||g.complete, value=done?g.target:g.value;
    let label=g.label.replace(/ in one run$| in one landed combo$| while this level is active$/, "");
    if (g.target===1) label=label.replace(/\b1 (backflips|elders|walls|balloons|loops|rocks|grinds|wingsuits|bounces)\b/g, (_,noun:string)=>`1 ${noun.slice(0,-1)}`);
    if (skin==="classic") s.riders.forEach((r,i)=>{ label=label.replaceAll(r.name,riderName(i)); });
    return `<article class="challenge-goal ${done?"complete":""}"><span class="goal-check" aria-label="${done?"Complete":"Incomplete"}">${done?"✓":"○"}</span><div><h4>${esc(label)}</h4><span class="goal-scope">${({run:"In one run",total:"Across runs",combo:"In one combo"} as Record<string,string>)[g.scope??"total"]}</span><div class="goal-progress"><progress value="${value}" max="${g.target}" aria-label="${esc(g.label)}"></progress><span>${Math.floor(value).toLocaleString()} / ${g.target.toLocaleString()}</span></div></div></article>`;
  }).join("");
  const next = s.riders.map((r,i)=>({r,i})).filter(({r})=>r.level>data.level).sort((a,b)=>a.r.level-b.r.level)[0]?.i ?? -1;
  el("challenge-unlock").textContent = data.complete ? "All 60 levels complete." : next<0 ? "All characters unlocked. Finish the final ten levels." : `Complete level ${s.riders[next].level-1} to unlock ${riderName(next)}.`;
  el("challenge-migration").hidden = !data.migrated || data.history.length>0;
  const chapter=Math.floor((challengeLevel-1)/10);
  el("challenge-chapters").innerHTML = Array.from({length:6},(_,i)=>`<button data-chapter="${i}" aria-pressed="${chapter===i}">${i*10+1}–${i*10+10}</button>`).join("");
  el("challenge-levels").innerHTML = Array.from({length:10},(_,i)=>{
    const n=chapter*10+i+1, state=n<data.level||data.complete?"complete":n===data.level?"current":"locked";
    return `<button data-level="${n}" class="${state}" aria-label="Level ${n}, ${state}" aria-pressed="${n===challengeLevel}"><strong>${n}</strong><small>${state=== "complete"?"✓ Done":state=== "current"?"Current":"Locked"}</small></button>`;
  }).join("");
}
function openChallenges() {
  if (mode === "play" || mode === "crash") dialog();
  challengeLevel = s.level;
  renderChallenges(); openPanel("challenge-panel");
}
el("challenges-open").onclick = openChallenges;
el("run-challenges").onclick = openChallenges;
el("challenge-current").onclick = ()=> { challengeLevel=s.level; renderChallenges(); };
el("challenge-chapters").onclick = e=> { const b=(e.target as Element).closest<HTMLButtonElement>("[data-chapter]"); if(b){challengeLevel=Number(b.dataset.chapter)*10+1;renderChallenges();el("challenge-chapters").querySelector<HTMLButtonElement>(`[data-chapter="${b.dataset.chapter}"]`)?.focus();} };
el("challenge-levels").onclick = e=> { const b=(e.target as Element).closest<HTMLButtonElement>("[data-level]");if(b){challengeLevel=Number(b.dataset.level);renderChallenges();el("challenge-levels").querySelector<HTMLButtonElement>(`[data-level="${challengeLevel}"]`)?.focus();} };
el("challenge-panel").querySelector<HTMLButtonElement>(".close-panel")!.onclick = closePanels;

