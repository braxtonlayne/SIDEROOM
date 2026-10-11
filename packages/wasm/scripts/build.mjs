// FND-REPO-005: build every crates/*-wasm crate with wasm-pack into packages/wasm/dist/<name>.
//
// Output is ESM (`--target web`): each module exports an async `default` init function plus the
// bindings, with generated `.d.ts`. The typed facade in src/index.js loads them lazily.
//
// Requirements: rustup target wasm32-unknown-unknown, `wasm-pack` (cargo install wasm-pack).
// Env: WASM_PROFILE=dev|release (default release), WASM_NO_OPT=1 skips wasm-opt (offline builds).
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, readdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(here, "..");
const root = resolve(pkgDir, "../..");
const dist = join(pkgDir, "dist");
const profile = process.env.WASM_PROFILE === "dev" ? "--dev" : "--release";
const noOpt = process.env.WASM_NO_OPT === "1";

const crates = readdirSync(join(root, "crates"), { withFileTypes: true })
  .filter((d) => d.isDirectory() && d.name.endsWith("-wasm"))
  .map((d) => d.name)
  .sort();

if (crates.length === 0) {
  console.error("No crates/*-wasm crates found.");
  process.exit(1);
}

const probe = spawnSync("wasm-pack", ["--version"], { stdio: "ignore", shell: true });
if (probe.status !== 0) {
  console.error("wasm-pack not found. Install it with `cargo install wasm-pack --locked`.");
  process.exit(1);
}

rmSync(dist, { recursive: true, force: true });

for (const crate of crates) {
  // sr-theme-wasm -> theme
  const name = crate.replace(/^sr-/, "").replace(/-wasm$/, "");
  const outDir = join(dist, name);
  console.log(`\n== wasm-pack: ${crate} -> dist/${name}`);
  const args = [
    "build",
    join(root, "crates", crate),
    "--target",
    "web",
    profile,
    "--out-dir",
    outDir,
    "--out-name",
    name,
    "--no-pack",
  ];
  if (noOpt) args.push("--no-opt");
  const r = spawnSync("wasm-pack", args, { stdio: "inherit", cwd: root, shell: true });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

// Ship the theme JSON Schema next to the bindings (source of truth: crates/sr-theme/schema).
const schema = join(root, "crates/sr-theme/schema/theme-package.schema.json");
if (existsSync(schema)) copyFileSync(schema, join(pkgDir, "theme.schema.json"));

console.log("\nBuilt:", crates.join(", "));
