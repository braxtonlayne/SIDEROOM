import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { errorKey, errorVars } from './errors';
import { readStored, useCopy, useToolsEngine, writeStored } from './hooks';
import './calc.css';

interface Entry {
  expr: string;
  text: string;
  value: number;
}
const HISTORY_KEY = 'sr.tools.calc.history';
const HISTORY_MAX = 20;
const isHistory = (v: unknown): v is Entry[] =>
  Array.isArray(v) &&
  v.every(
    (e) =>
      e &&
      typeof (e as Entry).expr === 'string' &&
      typeof (e as Entry).text === 'string' &&
      typeof (e as Entry).value === 'number',
  );

type KeyName = 'sqrt' | 'pi' | 'e' | 'power' | 'factorial' | 'percent' | 'mod' | 'ans';

/** Keys of the scientific keypad: what each button types, how it reads, and its accessible name. */
const KEYS: { insert: string; label: string; name: KeyName | null }[] = [
  { insert: 'sin(', label: 'sin', name: null },
  { insert: 'cos(', label: 'cos', name: null },
  { insert: 'tan(', label: 'tan', name: null },
  { insert: 'ln(', label: 'ln', name: null },
  { insert: 'log(', label: 'log', name: null },
  { insert: 'sqrt(', label: '√', name: 'sqrt' },
  { insert: '^', label: 'xʸ', name: 'power' },
  { insert: '!', label: 'n!', name: 'factorial' },
  { insert: 'pi', label: 'π', name: 'pi' },
  { insert: 'e', label: 'e', name: 'e' },
  { insert: '(', label: '(', name: null },
  { insert: ')', label: ')', name: null },
  { insert: '%', label: '%', name: 'percent' },
  { insert: ' mod ', label: 'mod', name: 'mod' },
  { insert: 'ans', label: 'ans', name: 'ans' },
];

const KEY_NAMES = {
  sqrt: 'tools.calc.key.sqrt',
  pi: 'tools.calc.key.pi',
  e: 'tools.calc.key.e',
  power: 'tools.calc.key.power',
  factorial: 'tools.calc.key.factorial',
  percent: 'tools.calc.key.percent',
  mod: 'tools.calc.key.mod',
  ans: 'tools.calc.key.ans',
} as const;

/** The scientific calculator: an expression box with a live answer, a keypad and a short history. */
export default function CalculatorTool({ host }: ToolProps) {
  const t = useT();
  const { engine, failed } = useToolsEngine();
  const [expr, setExpr] = useState('');
  const [degrees, setDegrees] = useState(true);
  const [history, setHistory] = useState<Entry[]>(() => readStored(HISTORY_KEY, [], isHistory));
  const input = useRef<HTMLInputElement>(null);
  const { copied, copy } = useCopy();
  const ans = history[0]?.value ?? null;

  const result = useMemo(() => {
    if (!engine || expr.trim() === '') return null;
    return engine.calcEval(expr, degrees, ans);
  }, [engine, expr, degrees, ans]);

  // Where a keypad press leaves the caret. It is placed in a layout effect right after the new expression
  // commits, before the next input event, never in a later frame: a deferred move could land after the member
  // had typed on and pull the caret back into the middle of what they typed.
  const pendingCaret = useRef<number | null>(null);
  useLayoutEffect(() => {
    const at = pendingCaret.current;
    if (at === null) return;
    pendingCaret.current = null;
    const el = input.current;
    el?.focus();
    el?.setSelectionRange(at, at);
  }, [expr]);

  const insert = (text: string) => {
    const el = input.current;
    const start = el?.selectionStart ?? expr.length;
    const end = el?.selectionEnd ?? expr.length;
    pendingCaret.current = start + text.length;
    setExpr(expr.slice(0, start) + text + expr.slice(end));
  };

  const commit = () => {
    if (!result || !result.ok) return;
    const next = [{ expr: expr.trim(), text: result.text, value: result.value }, ...history].slice(
      0,
      HISTORY_MAX,
    );
    setHistory(next);
    writeStored(HISTORY_KEY, next);
  };

  const clearHistory = () => {
    setHistory([]);
    writeStored(HISTORY_KEY, []);
  };

  if (failed)
    return (
      <p role="alert" className="sr-tl-muted">
        {t('tools.calc.engineFailed')}
      </p>
    );

  return (
    <div className="sr-calc" data-host={host}>
      <div className="sr-calc-row">
        <input
          ref={input}
          className="sr-calc-input"
          type="text"
          inputMode="text"
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          dir="ltr"
          aria-label={t('tools.calc.expression')}
          aria-describedby="sr-calc-result"
          placeholder={t('tools.calc.placeholder')}
          value={expr}
          onChange={(e) => setExpr(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              commit();
            }
          }}
        />
        <div role="radiogroup" aria-label={t('tools.calc.angleMode')} className="sr-calc-seg">
          {[true, false].map((d) => (
            <button
              key={String(d)}
              type="button"
              role="radio"
              aria-checked={degrees === d}
              className="sr-calc-seg-btn"
              onClick={() => setDegrees(d)}
            >
              {d ? t('tools.calc.degrees') : t('tools.calc.radians')}
            </button>
          ))}
        </div>
      </div>

      <div id="sr-calc-result" className="sr-calc-result" role="status" aria-live="polite">
        {!engine && expr.trim() !== '' ? (
          <span className="sr-tl-muted">{t('tools.loading')}</span>
        ) : result?.ok ? (
          <>
            <span className="sr-calc-eq" aria-hidden="true">
              =
            </span>
            <output className="sr-calc-value" dir="ltr">
              {result.text}
            </output>
            <button type="button" className="sr-tl-btn" onClick={() => copy('result', result.text)}>
              {copied === 'result' ? t('tools.calc.copied') : t('tools.calc.copy')}
            </button>
          </>
        ) : result ? (
          <span className="sr-calc-error">
            {t(errorKey(result.error.code), errorVars(result.error))}
          </span>
        ) : (
          <span className="sr-tl-muted">{t('tools.calc.hint')}</span>
        )}
      </div>

      <div className="sr-calc-keys" role="group" aria-label={t('tools.calc.keypad')}>
        {KEYS.map((k) => (
          <button
            key={k.insert}
            type="button"
            className="sr-calc-key"
            aria-label={k.name ? t(KEY_NAMES[k.name]) : undefined}
            onClick={() => insert(k.insert)}
          >
            {k.label}
          </button>
        ))}
        <button type="button" className="sr-calc-key" onClick={() => setExpr('')}>
          {t('tools.calc.clear')}
        </button>
      </div>

      <section aria-label={t('tools.calc.history')}>
        <div className="sr-calc-row">
          <h3 className="sr-calc-h">{t('tools.calc.history')}</h3>
          {history.length > 0 && (
            <button type="button" className="sr-tl-btn" onClick={clearHistory}>
              {t('tools.calc.clearHistory')}
            </button>
          )}
        </div>
        {history.length === 0 ? (
          <p className="sr-tl-muted">{t('tools.calc.historyEmpty')}</p>
        ) : (
          <ul className="sr-calc-history">
            {history.map((h, i) => (
              <li key={`${i}-${h.expr}`}>
                <button
                  type="button"
                  className="sr-calc-hist-btn"
                  title={t('tools.calc.reuse')}
                  onClick={() => {
                    setExpr(h.expr);
                    input.current?.focus();
                  }}
                >
                  <span dir="ltr">{h.expr}</span>
                  <span dir="ltr" className="sr-calc-hist-val">
                    = {h.text}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
