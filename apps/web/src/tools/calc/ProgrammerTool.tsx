import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { errorKey, errorVars } from './errors';
import { useCopy, useToolsEngine } from './hooks';
import './calc.css';

const WIDTHS = [8, 16, 32, 64] as const;
const BASES = [
  { base: 16, key: 'tools.programmer.baseHex' },
  { base: 10, key: 'tools.programmer.baseDec' },
  { base: 8, key: 'tools.programmer.baseOct' },
  { base: 2, key: 'tools.programmer.baseBin' },
] as const;

/** Programmer mode: integers of a fixed width with two's complement, bitwise operators and a bit grid. */
export default function ProgrammerTool({ host }: ToolProps) {
  const t = useT();
  const { engine, failed } = useToolsEngine();
  const [expr, setExpr] = useState('');
  const [bits, setBits] = useState<number>(32);
  const [signed, setSigned] = useState(true);
  const [base, setBase] = useState<number>(10);
  const { copied, copy } = useCopy();

  const result = useMemo(() => {
    if (!engine || expr.trim() === '') return null;
    return engine.programmerEval(expr, bits, signed, base);
  }, [engine, expr, bits, signed, base]);

  // The bit pattern the grid shows and edits: the current result, or zero.
  const pattern = result?.ok ? BigInt(result.views.unsigned) : 0n;
  const toggle = (i: number) => {
    const next = pattern ^ (1n << BigInt(i));
    setBase(16);
    setExpr(`0x${next.toString(16).toUpperCase()}`);
  };

  if (failed)
    return (
      <p role="alert" className="sr-tl-muted">
        {t('tools.calc.engineFailed')}
      </p>
    );

  const rows: { key: string; label: string; value: string }[] = result?.ok
    ? [
        { key: 'hex', label: t('tools.programmer.hex'), value: result.views.hex },
        { key: 'dec', label: t('tools.programmer.decimal'), value: result.views.signed },
        { key: 'udec', label: t('tools.programmer.unsigned'), value: result.views.unsigned },
        { key: 'oct', label: t('tools.programmer.octal'), value: result.views.oct },
        { key: 'bin', label: t('tools.programmer.binary'), value: result.views.bin },
      ]
    : [];

  return (
    <div className="sr-calc" data-host={host}>
      <input
        className="sr-calc-input"
        type="text"
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
        dir="ltr"
        aria-label={t('tools.programmer.expression')}
        placeholder={t('tools.programmer.placeholder')}
        value={expr}
        onChange={(e) => setExpr(e.target.value)}
      />

      <div className="sr-calc-opts">
        <div role="radiogroup" aria-label={t('tools.programmer.width')} className="sr-calc-seg">
          {WIDTHS.map((w) => (
            <button
              key={w}
              type="button"
              role="radio"
              aria-checked={bits === w}
              className="sr-calc-seg-btn"
              onClick={() => setBits(w)}
            >
              {w}
            </button>
          ))}
        </div>
        <label className="sr-calc-check">
          <input type="checkbox" checked={signed} onChange={(e) => setSigned(e.target.checked)} />
          {t('tools.programmer.signed')}
        </label>
        <label className="sr-calc-field">
          {t('tools.programmer.inputBase')}
          <select value={base} onChange={(e) => setBase(Number(e.target.value))}>
            {BASES.map((b) => (
              <option key={b.base} value={b.base}>
                {t(b.key)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div role="status" aria-live="polite" className="sr-calc-result">
        {!engine && expr.trim() !== '' ? (
          <span className="sr-tl-muted">{t('tools.loading')}</span>
        ) : result && !result.ok ? (
          <span className="sr-calc-error">
            {t(errorKey(result.error.code), errorVars(result.error))}
          </span>
        ) : result?.ok && result.wrapped ? (
          <span className="sr-calc-warn">{t('tools.programmer.wrapped', { bits })}</span>
        ) : (
          <span className="sr-tl-muted">{t('tools.programmer.hint')}</span>
        )}
      </div>

      {rows.length > 0 && (
        <dl className="sr-calc-views">
          {rows.map((r) => (
            <div key={r.key} className="sr-calc-view">
              <dt>{r.label}</dt>
              <dd dir="ltr">{r.value}</dd>
              <button
                type="button"
                className="sr-tl-btn"
                aria-label={t('tools.programmer.copyView', { name: r.label })}
                onClick={() => copy(r.key, r.value.replace(/ /g, ''))}
              >
                {copied === r.key ? t('tools.calc.copied') : t('tools.calc.copy')}
              </button>
            </div>
          ))}
        </dl>
      )}

      <fieldset className="sr-calc-bits">
        <legend>{t('tools.programmer.bits')}</legend>
        {Array.from({ length: bits / 8 }, (_, byteFromTop) => {
          const byte = bits / 8 - 1 - byteFromTop;
          return (
            <div key={byte} className="sr-calc-bitrow" dir="ltr">
              {Array.from({ length: 8 }, (_, k) => {
                const i = byte * 8 + 7 - k;
                const on = ((pattern >> BigInt(i)) & 1n) === 1n;
                return (
                  <button
                    key={i}
                    type="button"
                    className="sr-calc-bit"
                    aria-pressed={on}
                    aria-label={t('tools.programmer.bit', { n: i })}
                    onClick={() => toggle(i)}
                  >
                    {on ? '1' : '0'}
                  </button>
                );
              })}
            </div>
          );
        })}
      </fieldset>
    </div>
  );
}
