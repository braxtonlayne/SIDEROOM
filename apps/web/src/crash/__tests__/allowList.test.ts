import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { findLeaks } from '../../../../../scripts/check-crash-card.mjs';
import { buildCrashCard, normalizeFile, suspicious, TICK_INPUTS, MAX_TICKS } from '../allowList';

const fixture = JSON.parse(
  readFileSync(resolve(process.cwd(), '../../e2e/fixtures/crash-hostile.json'), 'utf8'),
) as {
  payload: unknown;
  needles: string[];
  expect: Record<string, unknown> & { ticks: unknown[] };
};

const NOW = 1_800_000_000_000;

describe('the crash card allow-list (client)', () => {
  it('feeds a payload stuffed with secrets through and keeps none of them', () => {
    const card = buildCrashCard(fixture.payload, NOW);
    expect(card).not.toBeNull();
    const text = JSON.stringify(card);
    for (const needle of fixture.needles) {
      expect(text, `the card still contains ${needle}`).not.toContain(needle);
    }
    // The secret-pattern grep of the built card (scripts/check-crash-card.mjs) finds nothing either.
    expect(findLeaks(JSON.stringify(card, null, 2))).toEqual([]);
  });

  it('builds exactly the card the server builds from the same fixture', () => {
    const card = buildCrashCard(fixture.payload, NOW);
    expect(card).toEqual({ ...fixture.expect, at: NOW });
  });

  it('has only the listed top-level fields', () => {
    const card = buildCrashCard(fixture.payload, NOW)!;
    expect(Object.keys(card).sort()).toEqual(
      ['at', 'caps', 'commit', 'engine', 'frames', 'kind', 'route', 'ticks', 'v'].sort(),
    );
  });

  it('sends nothing without a valid commit and kind', () => {
    expect(buildCrashCard(null, NOW)).toBeNull();
    expect(buildCrashCard({ kind: 'error' }, NOW)).toBeNull();
    expect(buildCrashCard({ commit: 'ABCDEF1', kind: 'error' }, NOW)).toBeNull();
    expect(buildCrashCard({ commit: 'abcdef1', kind: 'panic!' }, NOW)).toBeNull();
    expect(buildCrashCard({ commit: 'abcdef1', kind: 'error' }, NOW)).not.toBeNull();
  });

  it('keeps a route name and drops a URL', () => {
    const route = (r: string) =>
      buildCrashCard({ commit: 'abcdef1', kind: 'error', route: r }, NOW)!.route;
    expect(route('lounge.:guildId.:channelId')).toBe('lounge.:guildId.:channelId');
    expect(route('/lounge/12345678901234567/9?x=1')).toBe('unknown');
    expect(route('https://a.test/x')).toBe('unknown');
  });

  it('keeps the newest 60 ticks, as enums', () => {
    const ticks = Array.from({ length: 100 }, (_, i) => ({
      t: i,
      src: 'game',
      input: i % 2 ? 'free text with a secret' : 'left',
    }));
    const card = buildCrashCard({ commit: 'abcdef1', kind: 'error', ticks }, NOW)!;
    expect(card.ticks).toHaveLength(MAX_TICKS);
    expect(card.ticks[0]!.t).toBe(40);
    expect(card.ticks.every((t) => (TICK_INPUTS as readonly string[]).includes(t.input))).toBe(
      true,
    );
  });

  it('normalizes file names like the server', () => {
    expect(normalizeFile('https://h.test/assets/a-1b2c3d4e.js?x=1#y')).toBe('assets/a-1b2c3d4e.js');
    expect(normalizeFile('https://h.test/a/b/c/d/e/f.js')).toBe('c/d/e/f.js');
    expect(normalizeFile('https://h.test/x/550e8400-e29b-41d4-a716-446655440000/y.js')).toBe(
      'x/:id/y.js',
    );
    expect(normalizeFile('/home/bob/src/lib.rs')).toBe('home/:id/src/lib.rs');
    expect(normalizeFile('wasm://wasm/1234abcd')).toBe('wasm');
    expect(normalizeFile('https://h.test')).toBeNull();
    expect(normalizeFile('https://h.test/a b/c.js')).toBe(':id/c.js');
  });

  it('flags credential shapes and spares ordinary names', () => {
    for (const bad of [
      'a@b.co',
      '10.1.2.3',
      'AKIAIOSFODNN7EXAMPLE',
      'eyJhbGciOiJIUzI1NiJ9.abc',
      'https://x.test',
      'X-Amz-Signature',
      '0123456789abcdef0123456789abcdef',
    ]) {
      expect(suspicious(bad), bad).toBe(true);
    }
    for (const ok of [
      'index-Bx3k9Qa1.js',
      'renderChannel',
      'sr_core::mls::decrypt',
      'useAuthToken',
    ]) {
      expect(suspicious(ok), ok).toBe(false);
    }
  });
});
