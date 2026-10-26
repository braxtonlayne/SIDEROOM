// RDY-I18N-002: generates the `en-XA` pseudo-locale catalog from the English fragments, at build time.
// Accented letters, about 35 percent longer, wrapped in brackets so a string that is cut off at either end
// (or an English one that never went through t()) is visible on screen. ICU structure is kept: arguments,
// plural and select cases and `#` are untouched, only literal text changes.
//
// Usage: node scripts/pseudo-locale.mjs [--out catalog.json] [fragmentDir]
// The Vite plugin (apps/web/vite/pseudoLocale.ts) calls buildPseudoCatalog() and serves the result as the
// virtual module `virtual:sr-pseudo-locale`, only on a dev host or when SR_PSEUDO_LOCALE=1 is set for the build.
// apps/web/src/i18n/pseudo.ts does the same transform at runtime for `ar-XB`; pseudoLocale.test.ts proves
// the two agree on every key, so keep the alphabet and the padding rule identical in both.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const FROM = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ';
const TO = [...'àƀçðéƒĝĥîĵķļɱñöþǫŕšţûṽŵẋýžÀƁÇÐÉƑĜĤÎĴĶĻṀÑÖÞǪŔŠŢÛṼŴẊÝŽ'];

/** Accent the letters and pad about 35 percent. */
export function pseudoText(s) {
  const accented = [...s].map((c) => {
    const i = FROM.indexOf(c);
    return i < 0 ? c : TO[i];
  });
  const pad = '·'.repeat(Math.ceil(s.replace(/\s/g, '').length * 0.35));
  return accented.join('') + pad;
}

/** Put literal text back into ICU source form: quotes doubled, braces (and `#` in a plural) quoted. */
function escapeText(s, plural) {
  let out = s.replace(/'/g, "''").replace(/\{/g, "'{'").replace(/\}/g, "'}'");
  if (plural) out = out.replace(/#/g, "'#'");
  return out;
}

/**
 * Pseudo-localise one ICU message. Mirrors the parser in apps/web/src/i18n/icu.ts (apostrophe quoting,
 * `#` only inside plural cases) so both produce the same text nodes.
 */
export function pseudoMessage(src) {
  let i = 0;
  const fail = (why) => {
    throw new Error(`pseudo-locale: ${why} at ${i} in "${src}"`);
  };
  const ws = () => {
    while (/\s/.test(src[i] ?? '')) i++;
  };

  function parse(depth, plural) {
    let out = '';
    let text = '';
    const flush = () => {
      if (text) out += escapeText(pseudoText(text), plural);
      text = '';
    };
    while (i < src.length) {
      const c = src[i];
      if (c === "'") {
        const n = src[i + 1];
        if (n === "'") {
          text += "'";
          i += 2;
        } else if (n === '{' || n === '}' || (plural && n === '#')) {
          const end = src.indexOf("'", i + 1);
          if (end < 0) {
            text += src.slice(i + 1);
            i = src.length;
          } else {
            text += src.slice(i + 1, end).replace(/''/g, "'");
            i = end + 1;
          }
        } else {
          text += c;
          i++;
        }
      } else if (c === '{') {
        flush();
        i++;
        out += argument(depth);
      } else if (c === '}') {
        if (depth === 0) fail("unmatched '}'");
        break;
      } else if (c === '#' && plural) {
        flush();
        out += '#';
        i++;
      } else {
        text += c;
        i++;
      }
    }
    flush();
    return out;
  }

  function argument(depth) {
    const start = i - 1;
    const ident = () => {
      ws();
      const m = /^[A-Za-z0-9_.-]+/.exec(src.slice(i));
      if (!m) fail('expected a name');
      i += m[0].length;
      return m[0];
    };
    const name = ident();
    ws();
    if (src[i] === '}') {
      i++;
      return src.slice(start, i);
    }
    if (src[i] !== ',') fail("expected ',' or '}'");
    i++;
    const kind = ident();
    if (kind === 'number' || kind === 'date' || kind === 'time') {
      const end = src.indexOf('}', i);
      if (end < 0) fail('unclosed argument');
      i = end + 1;
      return src.slice(start, i);
    }
    if (kind !== 'plural' && kind !== 'selectordinal' && kind !== 'select')
      fail(`unknown type ${kind}`);
    ws();
    if (src[i] === ',') i++;
    let out = `{${name}, ${kind},`;
    for (;;) {
      ws();
      if (src[i] === '}') {
        i++;
        break;
      }
      const m = /^(offset:\s*\d+|=\d+|[^\s{}]+)/.exec(src.slice(i));
      if (!m) fail('expected a case selector');
      i += m[0].length;
      const key = m[0].replace(/\s+/g, '');
      if (key.startsWith('offset:')) {
        out += ` ${key}`;
        continue;
      }
      ws();
      if (src[i] !== '{') fail("expected '{'");
      i++;
      out += ` ${key} {${parse(depth + 1, kind !== 'select')}}`;
      if (src[i] !== '}') fail("expected '}'");
      i++;
    }
    return `${out}}`;
  }

  return `[${parse(0, false)}]`;
}

/** Reads every fragment of apps/web/src/i18n/messages/en into one key to English text object. */
export function readEnglishCatalog(dir) {
  const catalog = {};
  const owner = {};
  for (const file of readdirSync(dir)
    .filter((f) => f.endsWith('.ts'))
    .sort()) {
    const body = readFileSync(join(dir, file), 'utf8')
      .replace(/^\s*export\s+default\s+/m, 'module.exports = ')
      .replace(/\}\s*(?:satisfies\s+Record<string,\s*string>|as\s+const)\s*;?\s*$/m, '}');
    const sandbox = { module: { exports: {} } };
    vm.runInNewContext(body, sandbox, { filename: file });
    for (const [key, text] of Object.entries(sandbox.module.exports)) {
      if (key in catalog)
        throw new Error(`Duplicate i18n key "${key}" in ${owner[key]} and ${file}`);
      catalog[key] = text;
      owner[key] = file;
    }
  }
  return catalog;
}

export const defaultFragmentDir = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  'apps',
  'web',
  'src',
  'i18n',
  'messages',
  'en',
);

/** The whole en-XA catalog: every English key with its pseudo-localised text. */
export function buildPseudoCatalog(dir = defaultFragmentDir) {
  const out = {};
  for (const [key, text] of Object.entries(readEnglishCatalog(dir))) out[key] = pseudoMessage(text);
  return out;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const o = args.indexOf('--out');
  const outFile = o >= 0 ? args[o + 1] : undefined;
  if (o >= 0) args.splice(o, 2);
  const catalog = buildPseudoCatalog(args[0] ?? defaultFragmentDir);
  const json = `${JSON.stringify(catalog, null, 2)}\n`;
  if (outFile) writeFileSync(outFile, json);
  else process.stdout.write(json);
  console.error(`en-XA: ${Object.keys(catalog).length} keys`);
}
