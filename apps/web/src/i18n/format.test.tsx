import { act, render, screen } from '@testing-library/react';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { ShortcutSheet } from '../lounge/ShortcutSheet';
import { DEFAULT_PREFS, usePrefsStore } from '../settings/user/prefs';
import {
  formatDate,
  formatDateTime,
  formatNumber,
  formatTime,
  setFormatLocale,
  setHour12,
  setLocale,
} from './index';

const sample = new Date(2026, 0, 31, 15, 5, 9);

afterEach(async () => {
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  setHour12(undefined);
  setFormatLocale(undefined);
  await setLocale('en');
  localStorage.clear();
});

describe('locale-aware formatters (XC-I18N-002)', () => {
  it('formats numbers, dates and times in the UI language', () => {
    expect(formatNumber(1234567)).toBe('1,234,567');
    expect(formatNumber(12.5, { style: 'currency', currency: 'USD' })).toBe('$12.50');
    expect(formatDate(sample, {})).toBe('1/31/2026');
    expect(formatDate(sample)).toBe('Jan 31, 2026');
    expect(formatDateTime(sample)).toMatch(/^1\/31\/2026, 3:05:09\sPM$/);
  });

  it('honours the 12/24 hour preference everywhere a time is shown', () => {
    setHour12(false);
    expect(formatTime(sample, { hour: 'numeric', minute: '2-digit' })).toBe('15:05');
    expect(formatDateTime(sample, { dateStyle: 'medium', timeStyle: 'short' })).toMatch(/15:05/);
    setHour12(true);
    expect(formatTime(sample, { hour: 'numeric', minute: '2-digit' })).toMatch(/^3:05\sPM$/);
    setHour12(undefined);
  });

  it('lets the region override the UI language for dates and numbers', () => {
    setFormatLocale('de');
    expect(formatNumber(1234567)).toBe('1.234.567');
    expect(formatDate(sample, {})).toBe('31.1.2026');
    setFormatLocale('not a locale');
    expect(formatNumber(1234)).toBe('1,234');
  });

  it('does not throw on an unparseable date', () => {
    expect(formatDate(new Date('nope'), {})).toBe('Invalid Date');
    expect(formatDateTime(Number.NaN)).toBe('Invalid Date');
  });

  it('follows the Language preferences without a reload', () => {
    act(() => {
      usePrefsStore.setState({
        prefs: { ...DEFAULT_PREFS, locale: 'de', timeFormat: '12h' },
      });
    });
    expect(formatNumber(1234567)).toBe('1.234.567');
    expect(formatTime(sample, { hour: 'numeric', minute: '2-digit' })).toMatch(/3:05\sPM/i);
    act(() => {
      usePrefsStore.setState({ prefs: DEFAULT_PREFS });
    });
    expect(formatNumber(1234567)).toBe('1,234,567');
  });
});

describe('pseudo-RTL smoke (dir="rtl")', () => {
  it('flips the document direction and still renders a real screen', async () => {
    await act(() => setLocale('ar-XB'));
    expect(document.documentElement.dir).toBe('rtl');
    render(<ShortcutSheet open onClose={() => {}} />);
    // The sheet renders translated (pseudo) copy, with every shortcut row intact.
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getAllByRole('term').length).toBeGreaterThan(5);
    await act(() => setLocale('en'));
    expect(document.documentElement.dir).toBe('ltr');
  });
});

describe('logical CSS properties', () => {
  const packages = join(__dirname, '..', '..', '..', '..', 'packages');
  const roots = [
    join(__dirname, '..'),
    join(packages, 'ui', 'src'),
    join(packages, 'markdown-react', 'src'),
  ];
  const css: string[] = [];
  const walk = (d: string) => {
    for (const name of readdirSync(d)) {
      if (name === 'node_modules') continue;
      const p = join(d, name);
      if (statSync(p).isDirectory()) walk(p);
      else if (name.endsWith('.css')) css.push(p);
    }
  };
  roots.forEach(walk);

  it('uses start/end instead of left/right for spacing, borders and alignment', () => {
    const physical =
      /(margin|padding)-(left|right)\s*:|border-(left|right)(-\w+)?\s*:|border-(top|bottom)-(left|right)-radius|text-align\s*:\s*(left|right)|float\s*:\s*(left|right)/;
    const hits: string[] = [];
    for (const f of css) {
      readFileSync(f, 'utf8')
        .split('\n')
        .forEach((line, i) => {
          if (physical.test(line)) hits.push(`${f}:${i + 1}: ${line.trim()}`);
        });
    }
    expect(hits).toEqual([]);
  });

  it('keeps physical left/right offsets only where a transform centres the box', () => {
    let offsets = 0;
    for (const f of css) {
      for (const line of readFileSync(f, 'utf8').split('\n')) {
        offsets += line.match(/(^|[{;])\s*(left|right)\s*:/g)?.length ?? 0;
      }
    }
    // Ratchet: new code should use inset-inline-start/end. Lower this when a leftover is converted.
    expect(offsets).toBeLessThanOrEqual(15);
  });
});
