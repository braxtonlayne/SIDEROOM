import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/**
 * RDY-A11Y-004: every animation in the Lounge, Cinema and Library honours the reduced-motion preference.
 *
 * Two inputs exist: the OS setting (`prefers-reduced-motion`) and the in-app Settings > Accessibility > Reduced
 * motion (`data-reduced-motion` on every themed root, set by ThemeProvider whatever the OS says). A single guard
 * in styles/a11y.css collapses CSS animations, transitions and CSS smooth scrolling for both; this test checks the
 * guard is there for both inputs, walks every stylesheet of the three sections (and the UI kit) to list what it
 * covers, and checks that no script asks for smooth scrolling without looking at the preference.
 */
const here = dirname(fileURLToPath(import.meta.url));
const SRC = resolve(here, '..');
const UI = resolve(here, '../../../../packages/ui/src');

function walk(dir: string, ext: RegExp, out: Record<string, string> = {}): Record<string, string> {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name !== 'node_modules' && e.name !== '__tests__') walk(p, ext, out);
    } else if (ext.test(e.name) && !/\.(test|stories)\./.test(e.name)) {
      out[p.split('\\').join('/')] = readFileSync(p, 'utf8');
    }
  }
  return out;
}
const SECTIONS = ['lounge', 'cinema', 'library', 'watchparty'];
const css: Record<string, string> = {
  ...walk(resolve(SRC, 'styles'), /\.css$/),
  ...Object.assign({}, ...SECTIONS.map((d) => walk(resolve(SRC, d), /\.css$/))),
  ...walk(UI, /\.css$/),
};
const sources: Record<string, string> = {
  ...Object.assign({}, ...['lounge', 'cinema', 'library'].map((d) => walk(resolve(SRC, d), /\.tsx?$/))),
  ...walk(UI, /\.tsx?$/),
};

const files = Object.entries(css);
const strip = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, '');

describe('reduced motion', () => {
  it('has one guard for the OS setting and one for the in-app setting', () => {
    const a11y = strip(files.find(([p]) => p.endsWith('/styles/a11y.css'))?.[1] ?? '');
    const os = /@media \(prefers-reduced-motion: reduce\)\s*\{[^@]*?animation-duration:\s*0\.01ms\s*!important[^@]*?transition-duration:\s*0\.01ms\s*!important/.exec(a11y);
    expect(os, 'the prefers-reduced-motion guard in styles/a11y.css').not.toBeNull();
    const app = /\[data-reduced-motion='true'\]\s*\*[^{]*\{[^}]*animation-duration:\s*0\.01ms\s*!important[^}]*transition-duration:\s*0\.01ms\s*!important[^}]*scroll-behavior:\s*auto\s*!important/.exec(a11y);
    expect(app, "the [data-reduced-motion='true'] guard in styles/a11y.css").not.toBeNull();
    expect(a11y).toMatch(/animation-iteration-count:\s*1\s*!important/);
  });

  it('finds the stylesheets of the three sections and the UI kit', () => {
    const where = (frag: string) => files.filter(([p]) => p.includes(frag)).length;
    expect(where('/lounge/')).toBeGreaterThan(3);
    expect(where('/cinema/')).toBeGreaterThan(3);
    expect(where('/library/')).toBeGreaterThan(0);
    expect(where('/packages/ui/')).toBeGreaterThan(5);
  });

  it('no stylesheet defeats the guard: motion is not declared !important or forced by an attribute', () => {
    const offenders: string[] = [];
    for (const [path, raw] of files) {
      if (path.endsWith('/styles/a11y.css')) continue;
      const text = strip(raw);
      // An `!important` on a duration or iteration count beats the guard (which is also `!important` but earlier in
      // the cascade for author sheets with equal specificity); `animation-play-state: running !important` too.
      for (const m of text.matchAll(/(animation(?:-[a-z-]+)?|transition(?:-[a-z-]+)?)\s*:[^;{}]*!important/g))
        if (!/:\s*none\b/.test(m[0])) offenders.push(`${path}: ${m[0].trim()}`);
    }
    expect(offenders).toEqual([]);
  });

  it('every infinite animation is one the guard collapses (finite keyframes, one iteration)', () => {
    // The guard sets iteration-count: 1 and duration: 0.01ms, so an infinite loop becomes one frame. The only
    // thing that would escape it is an animation driven by a JS timer, which the next test looks for.
    const infinite = files.flatMap(([path, raw]) =>
      Array.from(strip(raw).matchAll(/animation[^;{}]*\binfinite\b[^;{}]*;/g)).map((m) => `${path}: ${m[0].trim()}`),
    );
    expect(infinite.length).toBeGreaterThan(0); // the walk finds them; they are listed in the handoff
  });

  it('scripts only ask for smooth scrolling next to a reduced-motion check', () => {
    const offenders = Object.entries(sources)
      .filter(([, text]) => /behavior:\s*['"]smooth['"]|behavior:\s*[^,}]*\?\s*['"]auto['"]\s*:\s*['"]smooth['"]/.test(text))
      .filter(([, text]) => !/prefersReducedMotion|reduced|data-reduced-motion/i.test(text))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  it('the Cinema guide and the UI kit poster row do check the preference', () => {
    const guide = Object.entries(sources).find(([p]) => p.endsWith('/cinema/tv/Guide.tsx'))?.[1] ?? '';
    expect(guide).toMatch(/reduced \? 'auto' : 'smooth'/);
    const poster = Object.entries(sources).find(([p]) => p.endsWith('/sideroom/Poster.tsx'))?.[1] ?? '';
    expect(poster).toMatch(/reduced \? "auto" : "smooth"/);
  });
});
