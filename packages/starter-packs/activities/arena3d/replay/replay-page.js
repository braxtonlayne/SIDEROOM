// @ts-check
// The browser half of the determinism check: runs the recorded script with the bundle's vendored Jolt and writes
// the final hash into <output id="hash">. When it is done, that element's data-state is "done" (or "error"),
// which is what a Playwright spec waits for before reading the text in Chromium, Firefox and WebKit.

import { loadJolt } from '../sim/jolt-loader.js';
import { runReplay } from './run.js';
import script from './script.js';

/** @param {string} id */
const out = (id) => /** @type {HTMLOutputElement} */ (document.getElementById(id));

async function main() {
  out('ua').textContent = navigator.userAgent;
  const t0 = performance.now();
  const J = await loadJolt();
  const r = runReplay(J, script);
  out('ms').textContent = `${Math.round(performance.now() - t0)} ms (including loading Jolt)`;
  out('ticks').textContent = String(r.ticks);
  out('exact').textContent = r.finalExact;
  /** @type {HTMLPreElement} */ (document.getElementById('checkpoints')).textContent = r.checkpoints
    .map((c) => `${String(c.tick).padStart(4)}  ${c.hash}  ${c.exact}`)
    .join('\n');
  const hash = out('hash');
  hash.textContent = r.final;
  hash.dataset.state = 'done';
}

main().catch((e) => {
  const hash = out('hash');
  hash.textContent = `error: ${e instanceof Error ? e.message : String(e)}`;
  hash.dataset.state = 'error';
});
