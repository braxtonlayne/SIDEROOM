// Jolt's types as globals, for JSDoc in sim/. A JSDoc `import('jolt-physics/...')` would put a bare specifier in
// the bundled source, which tools/bundle.mjs refuses (rightly: the browser could not resolve it); a .d.ts is
// never bundled.
import type Jolt from 'jolt-physics/wasm-compat';

declare global {
  /** The jolt-physics wasm-compat factory: call it to instantiate one independent Jolt module. */
  type ArenaJoltFactory = typeof Jolt;
  /** One initialised Jolt module (its own WASM instance and heap). */
  type ArenaJoltModule = Awaited<ReturnType<typeof Jolt>>;
}
export {};
