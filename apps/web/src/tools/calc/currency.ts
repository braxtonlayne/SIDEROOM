/** UTL-CALC-001: currency conversion over the ECB reference rates the server serves at `GET /tools/fx`. */

export interface FxRates {
  /** False when the instance switched the converter off (`tools.currency_rates`). */
  enabled: boolean;
  /** Always `EUR`: every rate is units of the currency per one euro. */
  base: string;
  /** The ECB reference date, `YYYY-MM-DD`; null before the first fetch. */
  as_of: string | null;
  /** Unix seconds the server stored the set; null before the first fetch. */
  fetched_at: number | null;
  /** No rates yet, or the newest fetch is older than `tools.currency_stale_hours`. */
  stale: boolean;
  rates: Record<string, number>;
}

/** Units of `code` per one euro. */
export function rateOf(fx: Pick<FxRates, 'base' | 'rates'>, code: string): number | null {
  if (code === fx.base) return 1;
  const r = fx.rates[code];
  return typeof r === 'number' && Number.isFinite(r) && r > 0 ? r : null;
}

/** Converts through the euro. Null when either currency is unknown or the amount is not a number. */
export function convertCurrency(
  amount: number,
  from: string,
  to: string,
  fx: Pick<FxRates, 'base' | 'rates'>,
): number | null {
  if (!Number.isFinite(amount)) return null;
  const a = rateOf(fx, from);
  const b = rateOf(fx, to);
  if (a === null || b === null) return null;
  const out = (amount / a) * b;
  return Number.isFinite(out) ? out : null;
}

/** The codes to offer: the euro first, then the rest alphabetically. */
export function currencyCodes(fx: Pick<FxRates, 'base' | 'rates'>): string[] {
  return [
    fx.base,
    ...Object.keys(fx.rates)
      .filter((c) => c !== fx.base)
      .sort(),
  ];
}

/** Parses what a person types as an amount: spaces and thousands commas are ignored, a comma decimal is read. */
export function parseAmount(text: string): number | null {
  const t = text.trim().replace(/[\s_]/g, '');
  if (t === '') return null;
  // "1,234.56" and "1.234,56" both occur; the last separator is the decimal one.
  const lastComma = t.lastIndexOf(',');
  const lastDot = t.lastIndexOf('.');
  let s = t;
  if (lastComma >= 0 && lastDot >= 0) {
    s = lastComma > lastDot ? t.replace(/\./g, '').replace(',', '.') : t.replace(/,/g, '');
  } else if (lastComma >= 0) {
    // "1,234" is a thousand, "1,5" and "12,34" are decimals.
    s = /^\d{1,3}(,\d{3})+$/.test(t) ? t.replace(/,/g, '') : t.replace(',', '.');
  }
  if (!/^-?\d*\.?\d+$|^-?\d+\.$/.test(s)) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}
