# Alto’s Tower

Rust/WASM downhill prototype. Classic view is the default reference baseline; Tower view changes the artwork while using exactly the same Rust simulation. The baseline is **not yet a verified 1:1 recreation** of Alto’s Adventure.

## Run

Node 22+, Rust with `wasm32-unknown-unknown`, and wasm-bindgen-cli 0.2.104 are required.

```sh
npm install
npm run assets
npm run build
npm run dev
```

Open http://127.0.0.1:5190. Rebuild WASM after Rust edits. Asset synchronization resolves the user's extracted Tower art through `thetowersdk/assets`; the supplied TowerTheGathering player portrait appears in Tower view.

## Controls and interactions

- Space / tap: jump; hold in air to flip after a short delay; release stops rotating; it does not correct an unfinished flip.
- W / Wingsuit: toggle flight after charging the scarf. Hold Jump to rotate the flight trajectory through climbs and loops; release to dive. Landed combos build the scarf; long grinds and wallrides refresh it. Coins do not charge it.
- Land upright on a cable to grind. Jump leaves the cable. Inverted crossings pass through it.
- Magnet attracts coins. Feather smoothly lifts the rider above rocks. Shield absorbs one rock hit or bad landing. None forces a jump or spin.
- P / Escape: pause. Backgrounding the tab also pauses.
- Zen recovers from crashes. This prototype still records mission progress in Zen.

Every run receives a fresh seed. Authored encounter templates compose stationary, continuous terrain, long cables, rocks, chasms, drops and coin lines. Odyssey-inspired wall faces, airborne balloons and moving balloon cables now join the route bag. Jump and hold inside a face to ride upward; release for a wallride kicker. Balloon bounces accept inverted contact and preserve the pending combo. Proximity flight earns distance-based points and raises terrain dust. Tower view currently animates enemy sprites cosmetically; it does not change terrain or collision.

## Reference status

Alto’s Adventure 1.8.24 (versionCode 500181, package `com.noodlecake.altosadventure`) was acquired from APKMirror for local analysis. The bundle SHA-256 matches the published value `ca67404759a4028f2ed8d20a45633ef100b2aca44716dc6e2e867218a1fa6a11`. All five APK publisher signatures and package/version manifests were verified in the offline CPU container. Static type recovery succeeded after correcting an IL2CPP assembly-name mismatch in the analysis tooling. Curated serialized gravity, character and movement fields are recorded in `reference/alto-adventure-1.8.24.json`, including source object identities. The Rust character profiles now use those recovered scalar values. A chosen scale of 40 canvas units per source unit gives Alto gravity 1000 and jump impulse 640; air drag uses an exact continuous integration shared with coin guides. Source jump strength as initial velocity, rotation factors as turns per second, and landing target size as a half-angle remain runtime interpretations to verify in the emulator.

The original game's official triple-backflip guide establishes that landed trick chains build speed, speed produces more air, and Maya rotates faster: https://altosadventure.com/support/triple_backflips.html . Actual gameplay footage was inspected for camera placement and fixed scenery. Serialized gravity, jump strength, character rotation factors, drag and double-jump flags now come from the verified reference. Ground acceleration and boost formulas, wingsuit flight, pickup durations and progression thresholds remain reconstructed choices. Wingsuit unlock/shop, elders, llamas, original mission sets, character gates, terrain distribution and scoring remain parity gaps.

APK analysis is staged separately at `E:/alto-reference`; it follows tower-extractor's CPU-only, offline container constraints with read-only inputs, bounded resources and local output. No APK, raw dump, native code or original artwork is committed to this project or transmitted for analysis.

Saved distance is now stored in meters; version 2 migrates the old pixel-based totals while preserving earned progress. Rails preserve entry momentum and the opening cable supports at least four seconds of continuous grinding across the tested seeds.

The user's 7m23s MuMu session of Adventure 1.8.27, playing Felipe, now supplies a visual calibration record in `reference/player-session-20261006.json`. The camera shows approximately 10.8 pixels per meter at 960×540 (previously 21.6), follows the airborne rider instead of the floor, and allows bounded forward drift. Its tracking integrator preserves lag across display refresh rates. Palette transitions now follow a slow clock rather than changing every 212 meters. Felipe has a quadruped silhouette in Classic view. Wingsuit climbing no longer cancels its own lift, and release changes direction smoothly while retaining forward momentum. Camera response and wingsuit ratios remain reconstructed estimates; absent input timestamps and sparse late video frames prevent an exact native physics fit. The earlier automated starting-village sequence was excluded as an isolated jump reference.

Rendering now interpolates across the full outer physics tick. Previously the internal collision substeps overwrote the previous pose, leaving only half of a 120Hz tick to interpolate and producing uneven motion at other display refresh rates.

## Verification

```sh
npm test
cargo clippy --manifest-path engine/Cargo.toml -- -D warnings
npm run build
npm run test:browser
```

Rust tests cover seed replay and bounded terrain, continuity, tap and held rotation, release behavior, clean flip banking, bad landings, one-way rail contact and actual sustained grinding from the opening jump across four seeds, beneficial pickup effects, feather expiration near rocks, shield consumption, wingsuit Jump/expiration across every rider, and saved progression gates. Browser tests exercise real controls and persistence. Passing these checks proves those interactions, not Alto parity.

Held rotation now uses a separate 0.65 calibration multiplier (35% slower), leaving recovered character factors unchanged. A named trick chain persists through pickups and banks visibly on landing. Banking applies an immediate 22–40% bounded speed kick and enables rock smashing at sufficient boosted speed. Felipe’s passive second jump is silent. Twelve shuffled encounter types add rock rhythms, village bunting, roof transfers, chasms, crest drops, ice, forests and ruins; their mandatory routes do not require powerups. Seeded chasm tests require a contiguous safe single-jump input window across all six character profiles at minimum and maximum speed. These are prototype tuning choices, not verified native parity.

Pending flip awards now use the captured directed180-degree world-angle gate, with points banked only after a safe landing. Airborne release preserves orientation. This small scoring tolerance does not widen the collision landing cone. The browser regression performs Tupa’s double jump, releases after the pose completes its rotation, then checks banked score and increased velocity. Trick changes update the HUD on the next rendered frame.

## Flight revision 5

The instrumented 4m54 good run provides native inputs, velocities, scarf state, scoring and video. A 646-pair free-flight fit gives horizontal drag 0.30036 and acceleration 2.0525, and vertical drag 0.29505 and downward acceleration 27.1894 in source units. The flight controller uses drag 0.30, rotates momentum while held, and allows complete loops. Flight expiry clears queued jump input. Scarf activation uses the recovered 0.8 normalized threshold, with charge retained until depletion; charge decay is a Felipe trace fit, not a recovered universal formula. The shop and original progression remain parity gaps.

`reference/alto-odyssey-1.0.42.json` records signature-verified APK parameters, wallride curves, balloon collision/bounce values, scoring and explicit unknowns. Wallride curve application and balloon impulse are reconstructions: contact formulas and force application have not been recovered from native methods. Proximity checks are distance based, supported by 631 Adventure samples where nextProximityCheck equals current x + 2. The Odyssey field has the same interval; no playable Odyssey trace exists.

The old video-only calibration limitations above apply to that earlier capture, not the new instrumented run. Aircraft-like pitch clamps and the previous six-second fuel timer are removed. Ground and rail landings preserve their orientation checks; balloons are explicitly non-crashable. Native tests exercise these input/contact transitions; browser checks cover real controls and scarf display. Neither proves 1:1 parity.

## Controller revision 6

`reference/native-economy-audit-20261007.json` adds conditional native execution checks for Odyssey 1.0.42. The radio count getter returns 2–6 for supplied upgrade values 0–4 with the serialized base/per-level settings. Reward allocation accumulates supplied weights, and selection scans cumulative thresholds using a float random range. These checks use explicit dependency fixtures: they do not establish purchase mapping, actual package counts, runtime reward ordering or drop chances. Large-coin payout remains unresolved. The engine and UI were not changed by this recovery pass.

`reference/economy-audit-20261006.json` verifies omitted economy features separately for the two extracted versions. Both have workshop products, SuperCoin prefabs and 60 native goal sets. Odyssey includes authored circle/diamond/square/triangle pattern presets and the radio's `crateDrop`/`crateDropLevel` products; Adventure uses llama-horn products. The prototype now has a deliberately balanced shop and big coins worth 10, as requested; geometric formations, radio crates and original mission progression remain absent. The economy recovery now resolves Coin and Powerup_CoinMagnet references in four Odyssey formation presets, and decodes selected Coin/SuperCoin/Radio components with complete byte consumption. Generated string-array nodes require normalization and UnityPy’s Python parser; its accelerated path fails with `read_str out of bounds`. Coin payout amounts are not serialized on those components. Radio `ItemDrop` settings include count base 2, count per level 1, rare-item range 0–1 and drop delay 1; its drop target resolves to `Item_Mystery_Container`. The mystery container’s configured rewards resolve to one magnet, one lotus flower or 10–30 coins. Its probability fields are 0.25/0.25/1; actual normalized chances and selection order remain unverified. Large-coin payout and pattern generator semantics also remain unresolved. Radio `initialPoolSize=5` is not proof of five care packages.

All 21 captured native wingsuit `TouchUp` snapshots retain nonzero `currentRotateSpeed` (80.645–250 degrees/s). Those immediate snapshots precede controller processing. Subsequent states show forward release zeroing turn speed; backward release completes the loop until playerAngle is below90 degrees, then zeros it. The controller now follows that release distinction; its velocity/visual-angle integration remains approximate.

The full implementation audit is in `reference/parity-audit-20261006.json`, with additional signature-verified elder fields and captured score events in `reference/parity-facts-20261006.json`. Earlier camera, terrain, coin and contact changes were reconstruction/design choices, not proof of Alto parity. The audit explicitly lists those gaps. Guessed new wingsuit tuning, double-jump expiration and all-rope break timers were withdrawn. Authoritative trick history now survives long chains and intervening bounce/grind events, fixing charge bookkeeping. Wingsuit toggles preserve held Jump. Rock-smash and elder-escape scores now use captured 50 and 800 values; ice uses the serialized 1.4 multiplier. The 2500m pursuit interval is the user's requested override: Adventure's extracted spacing is 2000m. Offscreen pursuers use a distance indicator instead of moving the enemy into view.

The instrumented capture now supplies 733 released airborne board pairs: rotation eases toward velocity heading with median gain 0.78460/s. It never creates a completed flip. Felipe is roster index 5; his observed held flip rate is 225 degrees/s. Scarf depletion was not constant: 8,163 pairs fit normalized loss approximately `.02 + .04 * length` per second. Forty observed growth phases lasted a median 1.2632 seconds and paused target depletion. The reconstruction uses a 1.25-second protected growth phase, smooth displayed length, discrete banked boost levels and an 80% displayed-length readiness threshold. Charge weights remain reconstructed.

Boost protection no longer depends on stale boarding speed during flight. A visible gold ring/trail and a countdown bar use the same active timer as rock protection. Board-first rock bounces keep pending combos; side and inverted rock impacts remain hazardous, following [Apple's contact guide](https://apps.apple.com/kg/story/id1370659296). Captured magnet coins retain attraction and converge relative to player motion, including from behind and after the pickup expires.

Coins use small fixed groups at route approaches, recovery points and selected cable sections, with empty stretches between them. Ground groups sit within ordinary pickup reach. Review of the user's good-run video at 130–382 seconds showed compact groups on terrain and cable routes, rather than a continuous blanket. These placements are a reconstruction, not an exact measured spawn distribution. Cliff faces use irregular rocky crests and shoulders behind the foreground terrain. Steep framing widens by at most 28%, shifts the rider lower and accounts for nearby trailing terrain; wall anticipation uses the same zoom limit. Exact terrain segments and retained trailing geometry preserve the visible hill. Rider size partly compensates for wider framing so rotation stays readable.

Jump guides are fixed seeded world objects. Four varied opening routes replace the forced cable opening; steep descent routes have a brief approximately 65-degree face and a long shallow runout, replacing the sustained 74-degree plunge. A wall-to-balloon-to-cable sequence is tested over nine launch/release combinations. Terrain samples use fixed world coordinates, canyon rendering uses exact gap boundaries, and camera forward anticipation stays continuous through takeoff and landing. An elder pursuit now starts at visible camps, reacts to earned speed and ends when a chasm is crossed. Its timing, chase speed and 200-point escape reward are prototype choices; no native chase calibration exists.

## Prototype shop

Big coins display 10 and award ten coins. They replace occasional coins on existing routes rather than adding dense new trails. Saved lifetime coins migrate to a separate spendable wallet; purchases never reduce lifetime mission totals.

The Rust catalog in engine/src/shop.rs uses the extracted six-level Magnet and Feather tables (250/500/1000/2500/5000/7500), five-level Wingsuit lining table (1000/2500/5000/10000/15000), and a Tower-only Scarf weave (500/1000/2500/5000/7500). Helmets cost1500 and rescue picks3000, with up to three of each carried. Both safety items activate automatically and are consumed only when used; Zen never consumes them. Workshop prices come from the original tables; Scarf weave remains a custom Tower upgrade. Upgrade effect scaling and inventory capacity are prototype choices.



Double jump is now limited to one second after takeoff, once per launch. The existing capture shows21 successful second-jump transitions, latest at0.839s. One second is explicit prototype tuning; the original upper cutoff remains unverified. Pending backflip mission counts survive wingsuit mode changes; wingsuit loops do not count as board backflips. Feather coin collection now uses rider/board extent rather than center-only distance.


## Presentation polish

Runs now kick off from the title village without replacing its world or rider: the world-space title scrolls away as the same seeded landscape starts moving. The first 0.45 seconds ease into full simulation speed; the entrance pose lasts up to 1.2 seconds, while camera framing settles independently over approximately 2.5 seconds based on the recorded village departure. Jump ends the pose without snapping this camera travel. Jump ends the entrance immediately and is applied in the same input event. A crash stops the Rust simulation, then animates a visual tumble with a separated board and impact dust for1.3seconds before showing results. Pause freezes presentation clocks; reduced motion uses brief fades/static crash poses. These timings are presentation choices, not recovered native constants.

Huts, ruins and rooftop facades sample terrain across their full footing rather than ending at a flat center anchor. Facades add shading, roof trim, windows and embedded plinths. Landing compression and push-off leg movement preserve board contact. The simulation/save state is not advanced by the crash animation.


Current polish pass (2026-10-07): Classic portraits use a simple flat atlas; the approved Tower atlas stays unchanged. Title idle, anticipation, board-mount hop and landing compression are separate cosmetic poses; Jump cancels the pose immediately. Terrain counterlean, airborne arms and landing knees react without changing collision rotation. Snow emission is time-based and bounded. Independent persisted Music and SFX toggles control the original local synthesized score and contact/trick effects, wind and grinding layers. Context creation/resumption requires a user gesture.

Workshop balance is now deliberately authored: permanent upgrades cost522000 coins in total. The final tier of every upgrade requires completing all60 levels plus100km of subsequent normal Play, then paying its coin price. Existing upgrades and funds remain valid. Bunting breaks after3seconds of continuous contact; roof/balloon cables differ. Course supports have explicit attachment identities, and ordinary downhill curvature retains ground contact while actual crests can launch the rider. Exact native physics and generator formulas remain unverified; these changes are not presented as an extracted1:1 implementation.

# Public validation

Public CI runs the engine tests without private reference material. The optional
`cargo test --manifest-path engine/Cargo.toml --features private-reference`
comparison requires the locally extracted reference JSON and remains a local check.
