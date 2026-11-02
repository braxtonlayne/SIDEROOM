// @ts-check
// Loads Jolt Physics in the browser, lazily (dynamic import), so the menu paints before ~3 MB of physics
// arrives. The bundle carries the single-threaded `wasm-compat` build (WASM embedded, no SharedArrayBuffer, no
// worker) at vendor/jolt-physics.wasm-compat.js; tools/bundle.mjs copies it there from node_modules. The CSP
// allows compiling it ('wasm-unsafe-eval') and nothing else.
//
// Node (tests, replay/run-node.mjs) does not use this file: it imports `jolt-physics/wasm-compat` directly
// and passes the factory in, so no bare specifier ends up in the bundle.

/** @typedef {import('./world.js').JoltFactory} JoltFactory */
/** @typedef {import('./world.js').JoltModule} JoltModule */

/** @type {Promise<JoltFactory> | null} */
let factory = null;

/**
 * The Jolt factory, imported once. Call it (`await (await joltFactory())()`) for each independent module.
 * @param {() => Promise<{ default: JoltFactory }>} [importer] where to import it from (tests can inject one)
 * @returns {Promise<JoltFactory>}
 */
export function joltFactory(importer) {
  if (!factory) {
    // The vendored file only exists in the bundle (dist/), not in the source tree.
    // @ts-ignore
    const load = importer ?? (() => import('../vendor/jolt-physics.wasm-compat.js'));
    factory = load().then((/** @type {{ default: JoltFactory }} */ m) => m.default);
  }
  return factory;
}

/**
 * One initialised Jolt module (its own WASM instance and heap).
 * @param {() => Promise<{ default: JoltFactory }>} [importer]
 * @returns {Promise<JoltModule>}
 */
export async function loadJolt(importer) {
  const init = await joltFactory(importer);
  return init();
}
