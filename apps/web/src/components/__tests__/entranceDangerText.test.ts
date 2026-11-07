import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { contrastRatio } from '@sideroom/ui';

/**
 * SEC-AUD-046 (D-577): danger text is one token per palette. The app themes have `--ui-danger-text`
 * (tokens-contrast.test.ts in packages/ui); the entrance keeps its own palette (D-385) and has
 * `--sr-entrance-danger-text`, which must reach 4.5:1 on every entrance surface. The refused-invite text and the
 * kit's error text read the tokens instead of mixing colours.
 */
const here = dirname(fileURLToPath(import.meta.url));
const read = (p: string) => readFileSync(resolve(here, p), 'utf8');
const entrance = read('../entrance.css');
const publicTokens = read('../../styles/tokens.css');
const kitInput = read('../../../../../packages/ui/src/primitives/Input/Input.module.css');

const value = (css: string, name: string) =>
  new RegExp(`${name}:\\s*(#[0-9a-fA-F]{6})`).exec(css)?.[1];

describe('danger text tokens (D-577)', () => {
  it('the entrance palette has a danger text colour at 4.5:1 or better on its surfaces', () => {
    const danger = value(entrance, '--sr-entrance-danger-text');
    expect(danger).toBeDefined();
    for (const surface of ['--sr-bg-0', '--sr-bg-1', '--sr-bg-2']) {
      const bg = value(publicTokens, surface);
      expect(bg, surface).toBeDefined();
      expect(contrastRatio(danger!, bg!), surface).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('the refused-invite text uses the entrance token, not a colour mix', () => {
    const rule = /\.ent-invite-msg\[data-tone='bad'\]\s*\{([^}]*)\}/.exec(entrance)?.[1] ?? '';
    expect(rule).toMatch(/color:\s*var\(--sr-entrance-danger-text\)/);
    expect(entrance).not.toMatch(/color-mix\([^)]*--ui-danger/);
  });

  it('the kit error text uses --ui-danger-text in every theme', () => {
    const rule = /\.error\s*\{([^}]*)\}/.exec(kitInput)?.[1] ?? '';
    expect(rule).toMatch(/color:\s*var\(--ui-danger-text/);
    expect(kitInput).not.toMatch(/color-mix/);
    expect(kitInput).not.toMatch(/data-theme/);
  });
});
