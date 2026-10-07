import { copyFileSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { gameAssetPath } from "thetowersdk/assets";
const source =
  process.env.TOWER_ASSETS_DIR ||
  resolve(
    "../TrackerWebsite/the-tower-run-tracker/packages/tower-assets/assets/game",
  );
const entries = [
  ...[
    "Basic",
    "Fast",
    "Tank",
    "Ranged",
    "Protector",
    "Vampire",
    "Boss",
    "Ray",
    "Scatter",
  ].map((n) => [n, `Enemy ${n}`, "enemies"]),
  ...[
    "Black Hole",
    "Chrono Field",
    "Death Wave",
    "Chain Lightning",
    "Swamp",
    "Spotlight",
  ].map((n) => [n, `Weapon ${n}`, "ultimate-weapons"]),
];
mkdirSync("public/art", { recursive: true });
const manifest = {};
for (const [key, name, domain] of entries) {
  const path = gameAssetPath(name, { domain, size: "md" });
  const file = key.toLowerCase().replaceAll(" ", "-") + ".webp";
  copyFileSync(resolve(source, path), resolve("public/art", file));
  manifest[key] = { url: `art/${file}`, source: path };
}
copyFileSync(
  process.env.TOWER_PLAYER_PORTRAIT ||
    resolve("../TowerTheGathering/src/ui/assets/portraits/player.webp"),
  "public/art/player.webp",
);
writeFileSync(
  "public/art/manifest.json",
  JSON.stringify(manifest, null, 2) + "\n",
);
console.log(
  `Copied ${entries.length} Tower SDK assets and supplied player portrait.`,
);
