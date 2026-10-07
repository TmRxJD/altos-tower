import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
function run(command, args) {
  const r = spawnSync(command, args, { stdio: "inherit" });
  if (r.error) throw r.error;
  if (r.status !== 0) process.exit(r.status ?? 1);
}
run(process.env.CARGO || "cargo", [
  "build",
  "--manifest-path",
  "engine/Cargo.toml",
  "--target",
  "wasm32-unknown-unknown",
  "--release",
  "--jobs",
  "1",
  "--target-dir",
  "engine/target",
]);
mkdirSync("src/wasm", { recursive: true });
run(process.env.WASM_BINDGEN || "wasm-bindgen", [
  "engine/target/wasm32-unknown-unknown/release/altos_tower.wasm",
  "--target",
  "web",
  "--out-dir",
  "src/wasm",
  "--out-name",
  "engine",
]);
