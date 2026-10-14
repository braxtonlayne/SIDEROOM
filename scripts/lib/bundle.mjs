import { posix } from 'node:path';

/** Static (non-dynamic) imports of a built chunk, as paths relative to dist. */
export function staticImports(file, code) {
  const out = [];
  const re =
    /(?:^|[;}\n,])\s*(?:import|export)\s*(?:[^'"();]*?\bfrom\s*)?["']\.\/([^"']+\.js)["']/g;
  for (const m of code.matchAll(re)) out.push(posix.join(posix.dirname(file), m[1]));
  return out;
}
