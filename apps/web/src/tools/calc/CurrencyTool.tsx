import { useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../../api/client';
import { formatDate, formatNumber, useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { convertCurrency, currencyCodes, parseAmount, rateOf, type FxRates } from './currency';
import { readStored, useCopy, writeStored } from './hooks';
import './calc.css';

const PAIR_KEY = 'sr.tools.currency.pair';
const isPair = (v: unknown): v is [string, string] =>
  Array.isArray(v) && v.length === 2 && v.every((x) => typeof x === 'string');

/** A money amount in `code`, or the plain number when the runtime does not know the code. */
function money(n: number, code: string): string {
  try {
    return formatNumber(n, { style: 'currency', currency: code });
  } catch {
    return `${formatNumber(n, { maximumFractionDigits: 4 })} ${code}`;
  }
}

/** Currency converter over the ECB reference rates the server keeps; the browser calls no third party. */
export default function CurrencyTool({ host }: ToolProps) {
  const t = useT();
  const [fx, setFx] = useState<FxRates | null>(null);
  const [failed, setFailed] = useState(false);
  const [amount, setAmount] = useState('100');
  const [pair, setPair] = useState<[string, string]>(() =>
    readStored(PAIR_KEY, ['EUR', 'USD'] as [string, string], isPair),
  );
  const { copied, copy } = useCopy();

  useEffect(() => {
    let live = true;
    apiFetch<FxRates>('/tools/fx', { retries: 0 }).then(
      (r) => live && setFx(r),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, []);

  const codes = useMemo(() => (fx ? currencyCodes(fx) : []), [fx]);
  const [from, to] = pair;
  const value = parseAmount(amount);
  const result = fx && value !== null ? convertCurrency(value, from, to, fx) : null;
  const unit = fx ? convertCurrency(1, from, to, fx) : null;

  const set = (next: [string, string]) => {
    setPair(next);
    writeStored(PAIR_KEY, next);
  };

  if (failed)
    return (
      <p role="alert" className="sr-calc-error">
        {t('tools.currency.failed')}
      </p>
    );
  if (!fx) return <p className="sr-tl-muted">{t('tools.loading')}</p>;
  if (!fx.enabled) return <p className="sr-tl-muted">{t('tools.currency.off')}</p>;
  if (codes.length < 2)
    return (
      <p role="status" className="sr-tl-muted">
        {t('tools.currency.empty')}
      </p>
    );

  // A saved pair may name a currency the feed no longer lists.
  const safeFrom = rateOf(fx, from) === null ? 'EUR' : from;
  const safeTo = rateOf(fx, to) === null ? (codes.find((c) => c !== safeFrom) ?? 'EUR') : to;

  return (
    <div className="sr-calc" data-host={host}>
      <label className="sr-calc-field">
        {t('tools.currency.amount')}
        <input
          className="sr-calc-input"
          type="text"
          inputMode="decimal"
          autoComplete="off"
          spellCheck={false}
          dir="ltr"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </label>
      <div className="sr-calc-convert">
        <label className="sr-calc-field">
          {t('tools.currency.from')}
          <select value={safeFrom} onChange={(e) => set([e.target.value, safeTo])}>
            {codes.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="sr-tl-btn sr-calc-swap"
          aria-label={t('tools.currency.swap')}
          onClick={() => set([safeTo, safeFrom])}
        >
          {'⇄'}
        </button>
        <label className="sr-calc-field">
          {t('tools.currency.to')}
          <select value={safeTo} onChange={(e) => set([safeFrom, e.target.value])}>
            {codes.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div role="status" aria-live="polite" className="sr-calc-result">
        {value === null && amount.trim() !== '' ? (
          <span className="sr-calc-error">{t('tools.currency.badAmount')}</span>
        ) : result !== null ? (
          <>
            <output className="sr-calc-value" dir="ltr">
              {money(result, safeTo)}
            </output>
            <button
              type="button"
              className="sr-tl-btn"
              onClick={() => copy('result', String(Math.round(result * 1e6) / 1e6))}
            >
              {copied === 'result' ? t('tools.calc.copied') : t('tools.calc.copy')}
            </button>
          </>
        ) : (
          <span className="sr-tl-muted">{t('tools.currency.hint')}</span>
        )}
      </div>

      {unit !== null && (
        <p className="sr-tl-muted" dir="ltr">
          {`1 ${safeFrom} = ${formatNumber(unit, { maximumSignificantDigits: 6 })} ${safeTo}`}
        </p>
      )}
      {fx.as_of && (
        <p className="sr-tl-muted">
          {t('tools.currency.asOf', {
            date: formatDate(new Date(`${fx.as_of}T12:00:00Z`), {
              dateStyle: 'medium',
              timeZone: 'UTC',
            }),
          })}
        </p>
      )}
      {fx.stale && (
        <p role="note" className="sr-calc-warn">
          {t('tools.currency.stale')}
        </p>
      )}
      <p className="sr-tl-muted">{t('tools.currency.source')}</p>
    </div>
  );
}
