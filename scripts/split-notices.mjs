#!/usr/bin/env node
// SEC-ENT-001 (D-385, entrance-isolation.md section 6): THIRD_PARTY_NOTICES.md splits in two.
//
//   dist/THIRD_PARTY_NOTICES.md       public: only the sections for software the PUBLIC surfaces actually ship
//   dist/app/THIRD_PARTY_NOTICES.md   gated: the full file (an inventory of the app), linked from the handbook
//
// A section (a `## ` heading) goes into the public copy when its package name appears in the public build's
// JavaScript (`dist/entrance/*.js`). The source is the full file that the Vite public directory copied into dist
// (a byte-identical copy of the repository root file; `check-notices-copy.mjs` keeps it so).
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Splits the notices into [header, ...sections], each section starting at its `## ` heading. */
export function splitSections(text) {
  const parts = text.split(/^(?=## )/m);
  return { header: parts[0], sections: parts.slice(1) };
}

/** `## mp4-muxer 5.2.1 and webm-muxer ...` gives `mp4-muxer`; the name a bundle would carry. */
export const sectionName = (section) => (/^## (\S+)/.exec(section)?.[1] ?? '').toLowerCase();

export function publicNotices(full, publicJs) {
  const { sections } = splitSections(full);
  const haystack = publicJs.toLowerCase();
  const shipped = sections.filter((s) => {
    const name = sectionName(s);
    return name.length > 2 && haystack.includes(name);
  });
  const head =
    '# Third-party notices\n\n' +
    'The public pages of this server ship only React, a router and the typefaces named in /fonts/README.md, ' +
    'all under permissive terms. Sections below, if any, are the software in those pages that asks for ' +
    'attribution.\n\n';
  return head + shipped.join('');
}

function main() {
  const dist = process.argv[2] ? resolve(process.argv[2]) : join(root, 'apps/web/dist');
  const file = join(dist, 'THIRD_PARTY_NOTICES.md');
  const full = existsSync(file)
    ? readFileSync(file, 'utf8')
    : readFileSync(join(root, 'THIRD_PARTY_NOTICES.md'), 'utf8');
  const entrance = join(dist, 'entrance');
  const js = existsSync(entrance)
    ? readdirSync(entrance)
        .filter((n) => n.endsWith('.js'))
        .map((n) => readFileSync(join(entrance, n), 'utf8'))
        .join('\n')
    : '';
  mkdirSync(join(dist, 'app'), { recursive: true });
  writeFileSync(join(dist, 'app', 'THIRD_PARTY_NOTICES.md'), full);
  writeFileSync(file, publicNotices(full, js));
  console.log('[split-notices] public copy at dist/, full file at dist/app/');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
