import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { argumentNames, parseIcu } from './icu';
import { en } from './messages/en';

const SRC = resolve(__dirname, '..');

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === 'node_modules' || name === '__tests__') continue;
      walk(p, out);
    } else if (/\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name)) out.push(p);
  }
  return out;
}

describe('English catalog', () => {
  it('every message is valid ICU', () => {
    for (const [k, v] of Object.entries(en)) expect(() => parseIcu(v), k).not.toThrow();
  });

  it('keys follow area.screen.thing', () => {
    for (const k of Object.keys(en)) expect(k, k).toMatch(/^[a-z0-9]+(\.[A-Za-z0-9]+)+$/);
  });

  it('every t("...") call with a literal key uses a key that exists', () => {
    const missing: string[] = [];
    const re = /\bt\(\s*(['"`])([a-z0-9][\w.]*)\1/g;
    for (const file of walk(SRC)) {
      for (const m of readFileSync(file, 'utf8').matchAll(re)) {
        const key = m[2]!;
        if (key.includes('.') && !(key in en))
          missing.push(`${file.slice(SRC.length + 1)}: ${key}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it('keys referenced as constants (for example in MessageKey tables) exist too', () => {
    const missing: string[] = [];
    const re = /'((?:entrance|settings|pwa|a11y)\.[A-Za-z0-9.]+)'/g;
    for (const file of walk(SRC)) {
      if (file.endsWith(join('messages', 'en.ts'))) continue;
      for (const m of readFileSync(file, 'utf8').matchAll(re)) {
        if (!(m[1]! in en)) missing.push(`${file.slice(SRC.length + 1)}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it('other catalogs only use keys and arguments that English defines', () => {
    const dir = join(SRC, 'i18n', 'messages');
    for (const f of readdirSync(dir)) {
      if (f === 'en.ts' || !f.endsWith('.ts')) continue;
      const src = readFileSync(join(dir, f), 'utf8');
      for (const m of src.matchAll(/^\s*'([\w.]+)':\s*(['"])(.*)\2,?$/gm)) {
        const [, key, , value] = m;
        expect(key! in en, `${f}: unknown key ${key}`).toBe(true);
        const want = [...argumentNames(parseIcu(en[key as keyof typeof en]))].sort();
        const got = [...argumentNames(parseIcu(value!))].sort();
        expect(got, `${f}: ${key} arguments`).toEqual(want);
      }
    }
  });
});
