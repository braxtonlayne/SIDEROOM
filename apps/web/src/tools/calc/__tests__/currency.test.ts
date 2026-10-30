import { describe, expect, it } from 'vitest';
import { en } from '../../../i18n/messages/en';
import { getTool, registryProblems, TOOLS } from '../../registry';
import { convertCurrency, currencyCodes, parseAmount, rateOf } from '../currency';

const fx = { base: 'EUR', rates: { USD: 1.08, GBP: 0.84, JPY: 160, CHF: 0.96 } };

describe('conversion through the euro', () => {
  it('knows the euro and the listed currencies', () => {
    expect(rateOf(fx, 'EUR')).toBe(1);
    expect(rateOf(fx, 'USD')).toBe(1.08);
    expect(rateOf(fx, 'XXX')).toBeNull();
    expect(rateOf({ base: 'EUR', rates: { BAD: 0 } }, 'BAD')).toBeNull();
    expect(rateOf({ base: 'EUR', rates: { BAD: Number.NaN } }, 'BAD')).toBeNull();
  });

  it('converts both ways and across', () => {
    expect(convertCurrency(100, 'EUR', 'USD', fx)).toBeCloseTo(108, 10);
    expect(convertCurrency(108, 'USD', 'EUR', fx)).toBeCloseTo(100, 10);
    expect(convertCurrency(216, 'USD', 'GBP', fx)).toBeCloseTo(168, 10);
    expect(convertCurrency(1, 'JPY', 'JPY', fx)).toBeCloseTo(1, 12);
    expect(convertCurrency(0, 'USD', 'GBP', fx)).toBe(0);
    expect(convertCurrency(-5, 'EUR', 'CHF', fx)).toBeCloseTo(-4.8, 10);
  });

  it('refuses what it cannot convert', () => {
    expect(convertCurrency(1, 'EUR', 'XXX', fx)).toBeNull();
    expect(convertCurrency(1, 'XXX', 'EUR', fx)).toBeNull();
    expect(convertCurrency(Number.NaN, 'EUR', 'USD', fx)).toBeNull();
    expect(convertCurrency(Number.POSITIVE_INFINITY, 'EUR', 'USD', fx)).toBeNull();
    expect(
      convertCurrency(Number.MAX_VALUE, 'JPY', 'CHF', {
        base: 'EUR',
        rates: { JPY: 1e-300, CHF: 1e300 },
      }),
    ).toBeNull();
  });

  it('lists the euro first and the rest in order', () => {
    expect(currencyCodes(fx)).toEqual(['EUR', 'CHF', 'GBP', 'JPY', 'USD']);
  });
});

describe('parsing an amount', () => {
  const cases: [string, number | null][] = [
    ['100', 100],
    ['  42 ', 42],
    ['1,234.56', 1234.56],
    ['1.234,56', 1234.56],
    ['1,234', 1234],
    ['1,5', 1.5],
    ['12,34', 12.34],
    ['1 234,5', 1234.5],
    ['1 234,5', 1234.5],
    ['1_000', 1000],
    ['.5', 0.5],
    ['5.', 5],
    ['-7.25', -7.25],
    ['', null],
    ['abc', null],
    ['1.2.3', null],
    ['--5', null],
    ['1e5', null],
    ['$5', null],
  ];
  it.each(cases)('%j is %j', (text, want) => {
    expect(parseAmount(text)).toBe(want);
  });
});

describe('the calculator tools in the registry', () => {
  const ids = ['calculator', 'programmer', 'units', 'currency', 'percent', 'timezone'];
  it('has all six, well formed, with English text', () => {
    expect(registryProblems()).toEqual([]);
    for (const id of ids) {
      const e = getTool(id);
      expect(e, id).toBeTruthy();
      expect(en[e!.nameKey], id).toBeTruthy();
      expect(en[e!.descriptionKey], id).toBeTruthy();
      expect(e!.aliases.length, id).toBeGreaterThan(2);
    }
    expect(TOOLS.filter((t) => ids.includes(t.id))).toHaveLength(6);
  });
  it('loads each tool lazily', async () => {
    for (const id of ids) {
      const mod = await getTool(id)!.load();
      expect(typeof mod.default, id).toBe('function');
    }
  });
  it('keeps catalog key segments free of underscores and hyphens', () => {
    const mine = Object.keys(en).filter((k) =>
      /^tools\.(calculator|calc|programmer|units|currency|percent|timezone)\./.test(k),
    );
    expect(mine.length).toBeGreaterThan(200);
    for (const k of mine) expect(k, k).toMatch(/^[A-Za-z0-9.]+$/);
  });
});
