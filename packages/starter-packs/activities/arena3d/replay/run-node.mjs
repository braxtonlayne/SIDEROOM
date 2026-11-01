// @ts-check
// The Node half of the determinism check: prints the same hashes replay/replay.html shows in a browser.
//   node replay/run-node.mjs           final hash, exact hash and the checkpoints
//   node replay/run-node.mjs --json    the same as JSON (for a script that compares browsers)

import initJolt from 'jolt-physics/wasm-compat';
import { runReplay } from './run.js';
import script from './script.js';

const J = await initJolt();
const t0 = performance.now();
const r = runReplay(J, script);
const ms = Math.round(performance.now() - t0);
if (process.argv.includes('--json')) {
  console.log(JSON.stringify({ final: r.final, finalExact: r.finalExact, ticks: r.ticks, checkpoints: r.checkpoints }));
} else {
  for (const c of r.checkpoints) console.log(`${String(c.tick).padStart(4)}  ${c.hash}  ${c.exact}`);
  console.log(`final ${r.final}  exact ${r.finalExact}  (${r.ticks} ticks, ${ms} ms, node ${process.versions.node})`);
}
