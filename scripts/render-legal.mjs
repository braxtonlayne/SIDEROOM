#!/usr/bin/env node
// SEC-ENT-001 (D-385, entrance-isolation.md section 7): renders the house notes as plain HTML, with no script.
//
//   dist/legal/house-notes.<locale>.html   one template per locale, served by sr-server (web.rs::legal)
//   dist/entrance/legal-<hash>.css         the one stylesheet (token custom properties only)
//
// The text comes from the `entrance.houseNotes.*` keys (and the sign-in answers of `entrance.support.*`). The
// template holds two placeholders the server fills, HTML-escaped, from the compliance settings:
//   {{operator_country}}  {{operator_contact}}
// Only English exists today; a locale without a template falls back to English at serve time.
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const messages = join(root, 'apps/web/src/i18n/messages/en');
const dist = process.argv[2] ? resolve(process.argv[2]) : join(root, 'apps/web/dist');

/** Reads a catalog fragment (`export default { ... } satisfies Record<string, string>`) without a TS toolchain. */
export function readFragment(file) {
  const text = readFileSync(file, 'utf8')
    .replace('export default', 'module.exports =')
    .replace(/satisfies Record<string, string>;?/, '');
  const m = { exports: {} };
  new Function('module', text)(m);
  return m.exports;
}

const esc = (s) =>
  String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

/** Catalog of every English fragment the legal page draws on. */
export function loadCatalog() {
  const cat = {};
  for (const f of ['entrance.ts', 'p30b-ent-build-1.ts'])
    Object.assign(cat, readFragment(join(messages, f)));
  return cat;
}

const CSS = `/* House notes (plain HTML, no script): token custom properties only. */
:root{--bg:#111110;--bg-1:#161613;--line:#2a2b24;--text:#ece6d6;--text-2:#a9a493;--gold:#c8a96a;color-scheme:dark}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--text);font:17px/1.6 Newsreader,Georgia,serif}
main{max-width:46rem;margin:0 auto;padding:48px 24px 80px}
h1{font-weight:400;font-size:2.4rem;margin:0 0 .6em}
h2{font-weight:400;font-size:1.5rem;margin:2em 0 .4em;border-top:1px solid var(--line);padding-top:1em}
p,li{color:var(--text-2)}
a{color:var(--gold)}
nav{padding:20px 24px;border-bottom:1px solid var(--line)}
nav a{text-decoration:none;letter-spacing:.14em;font-size:.8rem;text-transform:uppercase}
footer{padding:24px;border-top:1px solid var(--line);color:var(--text-2);font-size:.85rem;text-align:center}
`;

export function renderHouseNotes(cat, cssHref) {
  const t = (k) => {
    if (!(k in cat)) throw new Error(`render-legal: missing catalog key ${k}`);
    return cat[k];
  };
  const h = (k) => esc(t(k));
  const country = esc(t('entrance.houseNotes.privacy.operatorCountry')).replace(
    '{country}',
    '{{operator_country}}',
  );
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>${h('entrance.houseNotes.title')}</title>
<link rel="stylesheet" href="${cssHref}">
</head>
<body>
<nav><a href="/">${h('entrance.shell.home')}</a></nav>
<main>
<h1>${h('entrance.houseNotes.title')}</h1>
<p>${h('entrance.houseNotes.intro')}</p>
<section aria-labelledby="notes-rules" id="rules">
<h2 id="notes-rules">${h('entrance.houseNotes.rules.title')}</h2>
<ol>
${[1, 2, 3, 4, 5].map((n) => `<li>${h(`entrance.houseNotes.rules.${n}`)}</li>`).join('\n')}
</ol>
</section>
<section aria-labelledby="notes-privacy" id="privacy">
<h2 id="notes-privacy">${h('entrance.houseNotes.privacy.title')}</h2>
<p>${h('entrance.houseNotes.privacy.body1')}</p>
<p>${h('entrance.houseNotes.privacy.body2')}</p>
<p>${h('entrance.houseNotes.privacy.body3')}</p>
<p>${country}</p>
<p>${h('entrance.houseNotes.privacy.operatorContact')} <a href="mailto:{{operator_contact}}">{{operator_contact}}</a></p>
</section>
<section aria-labelledby="notes-terms" id="terms">
<h2 id="notes-terms">${h('entrance.houseNotes.terms.title')}</h2>
<p>${h('entrance.houseNotes.terms.body')}</p>
</section>
<section aria-labelledby="notes-credits" id="image-credits">
<h2 id="notes-credits">${h('entrance.houseNotes.credits.title')}</h2>
<p>${h('entrance.houseNotes.credits.door')}</p>
<p>${h('entrance.houseNotes.credits.fonts')} <a href="/fonts/README.md">${h('entrance.houseNotes.credits.licences')}</a></p>
</section>
<section aria-labelledby="notes-signin" id="signin">
<h2 id="notes-signin">${h('entrance.support.signin.title')}</h2>
<p>${h('entrance.support.signin.body')}</p>
<p>${h('entrance.support.twofa.body')}</p>
</section>
</main>
<footer>${h('entrance.shell.invitationOnly')}</footer>
</body>
</html>
`;
}

function main() {
  const cat = loadCatalog();
  const hash = createHash('sha256').update(CSS).digest('hex').slice(0, 8);
  const cssName = `legal-${hash}.css`;
  mkdirSync(join(dist, 'entrance'), { recursive: true });
  mkdirSync(join(dist, 'legal'), { recursive: true });
  writeFileSync(join(dist, 'entrance', cssName), CSS);
  writeFileSync(
    join(dist, 'legal', 'house-notes.en.html'),
    renderHouseNotes(cat, `/entrance/${cssName}`),
  );
  console.log(`[render-legal] dist/legal/house-notes.en.html + dist/entrance/${cssName}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
