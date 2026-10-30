#!/usr/bin/env node
// BYOS-UI-001: the storage screens are skin-agnostic (spec directive 2), so their CSS and inline styles may use
// semantic custom properties only. Fails on a raw colour (hex, rgb/hsl/hwb/lab/lch/oklch/oklab/color()/color-mix with
// literals, or a named colour) in the files below, and on a physical margin/padding/inset/border side where a logical
// property exists. Usage: node scripts/check-css-tokens.mjs [file ...]
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const WEB = join(ROOT, 'apps/web/src');
/** Directories (relative to apps/web/src) and files that belong to the BYOS UI. */
export const TARGETS = ['settings/user/storage', 'settings/server/storage', 'storage', 'player'];
const EXT = /\.(css|tsx?)$/;

const NAMED = [
  'black',
  'white',
  'red',
  'green',
  'blue',
  'yellow',
  'orange',
  'purple',
  'pink',
  'gray',
  'grey',
  'brown',
  'cyan',
  'magenta',
  'teal',
  'navy',
  'maroon',
  'olive',
  'lime',
  'aqua',
  'silver',
  'gold',
  'indigo',
  'violet',
];
const PHYSICAL =
  /(^|[\s;{])(margin|padding)-(left|right|top|bottom)\s*:|(^|[\s;{])(left|right|top|bottom)\s*:|border-(left|right|top|bottom)(-[a-z]+)?\s*:/;

/** Returns `{ line, rule, text }` for every violation in one file's text. */
export function scan(text, file = '') {
  const css = file.endsWith('.css');
  const out = [];
  text.split('\n').forEach((raw, i) => {
    // Drop comments and (in code) string-free noise; keep it simple and line based.
    const line = raw.replace(/\/\*.*?\*\//g, '').replace(/^\s*\/\/.*$/, '');
    const hit = (rule) => out.push({ line: i + 1, rule, text: raw.trim() });
    if (css || /style=|style:|color|background/i.test(line)) {
      if (/#[0-9a-fA-F]{3,8}\b(?![\w-])/.test(line) && !/&#|href|id=|#\w+['"`]\)/.test(line))
        hit('hex colour');
      if (/\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(\s*[\d.#-]/.test(line))
        hit('colour function');
      const prop = line.match(
        /(?:^|[\s{;])(?:color|background(?:-color)?|border(?:-[a-z]+)*|outline(?:-color)?|fill|stroke|box-shadow|text-shadow)\s*:\s*([^;]*)/,
      );
      if (prop) {
        const value = prop[1].replace(/var\([^)]*\)/g, '');
        for (const n of NAMED)
          if (new RegExp(`(^|[\\s,(])${n}($|[\\s,)])`, 'i').test(value)) hit(`named colour "${n}"`);
      }
    }
    if (css && PHYSICAL.test(line)) hit('physical property (use the logical one)');
  });
  return out;
}

function walk(p) {
  const s = statSync(p);
  if (s.isFile()) return EXT.test(p) ? [p] : [];
  return readdirSync(p).flatMap((n) => (n === '__tests__' ? [] : walk(join(p, n))));
}

export function targetFiles() {
  return TARGETS.flatMap((t) => walk(join(WEB, t))).filter((f) => !/\.test\.tsx?$/.test(f));
}

function main() {
  const files = process.argv.length > 2 ? process.argv.slice(2) : targetFiles();
  let bad = 0;
  for (const f of files) {
    for (const v of scan(readFileSync(f, 'utf8'), f)) {
      bad++;
      console.error(`${relative(ROOT, f)}:${v.line}: ${v.rule}: ${v.text}`);
    }
  }
  if (bad) {
    console.error(
      `check-css-tokens: ${bad} problem(s). Use var(--ui-*) tokens and logical properties.`,
    );
    process.exit(1);
  }
  console.log(`check-css-tokens: ${files.length} file(s) clean.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
