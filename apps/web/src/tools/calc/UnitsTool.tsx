import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { errorKey, errorVars } from './errors';
import { useCopy, useToolsEngine } from './hooks';
import { UNIT_NAME_KEYS } from './unitNames';
import './calc.css';

const CATEGORY_KEYS = {
  length: 'tools.units.cat.length',
  mass: 'tools.units.cat.mass',
  volume: 'tools.units.cat.volume',
  temperature: 'tools.units.cat.temperature',
  data: 'tools.units.cat.data',
  speed: 'tools.units.cat.speed',
  time: 'tools.units.cat.time',
  area: 'tools.units.cat.area',
} as const;
type CategoryId = keyof typeof CATEGORY_KEYS;

/** The first two units shown for a category before the member picks. */
const DEFAULTS: Record<string, [string, string]> = {
  length: ['km', 'mi'],
  mass: ['kg', 'lb'],
  volume: ['l', 'gal'],
  temperature: ['c', 'f'],
  data: ['gb', 'mb'],
  speed: ['kmh', 'mph'],
  time: ['h', 'min'],
  area: ['m2', 'ft2'],
};

/** Unit conversion: pick a category, a value and two units, or type "5 km to mi". */
export default function UnitsTool({ host }: ToolProps) {
  const t = useT();
  const { engine, failed } = useToolsEngine();
  const [category, setCategory] = useState<CategoryId>('length');
  const [from, setFrom] = useState('km');
  const [to, setTo] = useState('mi');
  const [valueText, setValueText] = useState('1');
  const [query, setQuery] = useState('');
  const [showAll, setShowAll] = useState(false);
  const { copied, copy } = useCopy();

  const listing = useMemo(() => engine?.unitsList() ?? null, [engine]);
  const units = listing?.categories.find((c) => c.category === category)?.units ?? [];

  const value = useMemo(() => {
    if (!engine) return null;
    if (valueText.trim() === '') return null;
    return engine.calcEval(valueText, false, null);
  }, [engine, valueText]);

  const converted = useMemo(() => {
    if (!engine || !value || !value.ok) return null;
    return engine.unitsConvert(value.value, from, to);
  }, [engine, value, from, to]);

  const all = useMemo(() => {
    if (!engine || !showAll || !value || !value.ok) return null;
    return engine.unitsConvertAll(value.value, from);
  }, [engine, showAll, value, from]);

  const parsed = useMemo(() => {
    if (!engine || query.trim() === '') return null;
    return engine.unitsQuery(query);
  }, [engine, query]);

  const pickCategory = (c: CategoryId) => {
    setCategory(c);
    const [a, b] = DEFAULTS[c] ?? ['', ''];
    setFrom(a);
    setTo(b);
  };

  const applyQuery = () => {
    if (!parsed || !parsed.ok) return;
    setCategory(parsed.category as CategoryId);
    setFrom(parsed.from);
    setTo(parsed.to);
    setValueText(String(parsed.value));
    setQuery('');
  };

  const label = (id: string, symbol: string) => {
    const key = UNIT_NAME_KEYS[id];
    return key ? (symbol ? `${t(key)} (${symbol})` : t(key)) : symbol;
  };

  if (failed)
    return (
      <p role="alert" className="sr-tl-muted">
        {t('tools.calc.engineFailed')}
      </p>
    );

  return (
    <div className="sr-calc" data-host={host}>
      <label className="sr-calc-field">
        {t('tools.units.quick')}
        <input
          className="sr-calc-input"
          type="text"
          autoComplete="off"
          spellCheck={false}
          dir="ltr"
          placeholder="5 km to mi"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              applyQuery();
            }
          }}
        />
      </label>
      {parsed && (
        <p
          role="status"
          aria-live="polite"
          className={parsed.ok ? 'sr-calc-value-line' : 'sr-calc-error'}
        >
          {parsed.ok
            ? t('tools.units.queryResult', {
                value: parsed.value,
                from: label(parsed.from, ''),
                result: parsed.text,
                to: label(parsed.to, ''),
              })
            : t(errorKey(parsed.error.code), errorVars(parsed.error))}
        </p>
      )}

      <label className="sr-calc-field">
        {t('tools.units.category')}
        <select value={category} onChange={(e) => pickCategory(e.target.value as CategoryId)}>
          {(Object.keys(CATEGORY_KEYS) as CategoryId[]).map((c) => (
            <option key={c} value={c}>
              {t(CATEGORY_KEYS[c])}
            </option>
          ))}
        </select>
      </label>

      <div className="sr-calc-convert">
        <label className="sr-calc-field">
          {t('tools.units.value')}
          <input
            className="sr-calc-input"
            type="text"
            inputMode="decimal"
            autoComplete="off"
            spellCheck={false}
            dir="ltr"
            value={valueText}
            onChange={(e) => setValueText(e.target.value)}
          />
        </label>
        <label className="sr-calc-field">
          {t('tools.units.from')}
          <select value={from} onChange={(e) => setFrom(e.target.value)}>
            {units.map((u) => (
              <option key={u.id} value={u.id}>
                {label(u.id, u.symbol)}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="sr-tl-btn sr-calc-swap"
          aria-label={t('tools.units.swap')}
          onClick={() => {
            setFrom(to);
            setTo(from);
          }}
        >
          {'⇄'}
        </button>
        <label className="sr-calc-field">
          {t('tools.units.to')}
          <select value={to} onChange={(e) => setTo(e.target.value)}>
            {units.map((u) => (
              <option key={u.id} value={u.id}>
                {label(u.id, u.symbol)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div role="status" aria-live="polite" className="sr-calc-result">
        {value && !value.ok ? (
          <span className="sr-calc-error">
            {t(errorKey(value.error.code), errorVars(value.error))}
          </span>
        ) : converted?.ok ? (
          <>
            <output className="sr-calc-value" dir="ltr">
              {converted.text}
            </output>
            <span className="sr-tl-muted">{units.find((u) => u.id === to)?.symbol}</span>
            <button
              type="button"
              className="sr-tl-btn"
              onClick={() => copy('result', converted.text)}
            >
              {copied === 'result' ? t('tools.calc.copied') : t('tools.calc.copy')}
            </button>
          </>
        ) : converted ? (
          <span className="sr-calc-error">
            {t(errorKey(converted.error.code), errorVars(converted.error))}
          </span>
        ) : (
          <span className="sr-tl-muted">{t('tools.units.hint')}</span>
        )}
      </div>

      <label className="sr-calc-check">
        <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
        {t('tools.units.showAll')}
      </label>
      {all?.ok && (
        <table className="sr-calc-table">
          <caption className="sr-calc-sr-only">{t('tools.units.allCaption')}</caption>
          <tbody>
            {all.rows.map((r) => {
              const u = units.find((x) => x.id === r.id);
              return (
                <tr key={r.id} aria-current={r.id === to ? 'true' : undefined}>
                  <th scope="row">{label(r.id, u?.symbol ?? '')}</th>
                  <td dir="ltr">{r.text}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
