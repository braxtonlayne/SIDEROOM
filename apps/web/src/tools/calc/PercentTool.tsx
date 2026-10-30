import { useMemo, useState } from 'react';
import { formatNumber, useT } from '../../i18n';
import type { ToolProps } from '../registry';
import type { PercentKind, PercentResult } from './engine';
import { errorKey, errorVars } from './errors';
import { useCopy, useToolsEngine } from './hooks';
import './calc.css';

const KINDS: PercentKind[] = ['of', 'whatpercent', 'ofwhat', 'add', 'sub', 'change'];

const KIND_LABELS = {
  of: 'tools.percent.kind.of',
  whatpercent: 'tools.percent.kind.whatpercent',
  ofwhat: 'tools.percent.kind.ofwhat',
  add: 'tools.percent.kind.add',
  sub: 'tools.percent.kind.sub',
  change: 'tools.percent.kind.change',
} as const;

const FIELD_A = {
  of: 'tools.percent.field.percent',
  whatpercent: 'tools.percent.field.part',
  ofwhat: 'tools.percent.field.part',
  add: 'tools.percent.field.amount',
  sub: 'tools.percent.field.amount',
  change: 'tools.percent.field.from',
} as const;

const FIELD_B = {
  of: 'tools.percent.field.of',
  whatpercent: 'tools.percent.field.whole',
  ofwhat: 'tools.percent.field.percent',
  add: 'tools.percent.field.percentToAdd',
  sub: 'tools.percent.field.percentToSubtract',
  change: 'tools.percent.field.to',
} as const;

const SENTENCES = {
  of: 'tools.percent.sentence.of',
  whatpercent: 'tools.percent.sentence.whatpercent',
  ofwhat: 'tools.percent.sentence.ofwhat',
  add: 'tools.percent.sentence.add',
  sub: 'tools.percent.sentence.sub',
  change: 'tools.percent.sentence.change',
} as const;

/** Phrases the parser reads (its grammar is English), so they are data, not translated text. */
const EXAMPLES = ['15% of 80', '80 + 15%', '20% off 50', '12 is what % of 60', 'from 50 to 65'];

const num = (n: number) => formatNumber(n, { maximumFractionDigits: 12 });

/** Percentage helper: type a phrase ("15% of 80", "80 + 15%") or fill the form. */
export default function PercentTool({ host }: ToolProps) {
  const t = useT();
  const { engine, failed } = useToolsEngine();
  const [phrase, setPhrase] = useState('');
  const [kind, setKind] = useState<PercentKind>('of');
  const [aText, setAText] = useState('');
  const [bText, setBText] = useState('');
  const { copied, copy } = useCopy();

  const fromPhrase = useMemo(() => {
    if (!engine || phrase.trim() === '') return null;
    return engine.percentPhrase(phrase);
  }, [engine, phrase]);

  const fromForm = useMemo((): PercentResult | null => {
    if (!engine || aText.trim() === '' || bText.trim() === '') return null;
    const a = engine.calcEval(aText, false, null);
    if (!a.ok) return a;
    const b = engine.calcEval(bText, false, null);
    if (!b.ok) return b;
    return engine.percentCompute(kind, a.value, b.value);
  }, [engine, kind, aText, bText]);

  const show = (r: PercentResult | null, id: string) => {
    if (!r) return null;
    if (!r.ok)
      return <p className="sr-calc-error">{t(errorKey(r.error.code), errorVars(r.error))}</p>;
    return (
      <p className="sr-calc-sentence" role="status" aria-live="polite">
        <span dir="auto">{t(SENTENCES[r.kind], { a: num(r.a), b: num(r.b), value: r.text })}</span>
        <button type="button" className="sr-tl-btn" onClick={() => copy(id, r.text)}>
          {copied === id ? t('tools.calc.copied') : t('tools.calc.copy')}
        </button>
      </p>
    );
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
        {t('tools.percent.phrase')}
        <input
          className="sr-calc-input"
          type="text"
          autoComplete="off"
          spellCheck={false}
          dir="ltr"
          placeholder="15% of 80"
          value={phrase}
          onChange={(e) => setPhrase(e.target.value)}
        />
      </label>
      {show(fromPhrase, 'phrase')}
      {!phrase && (
        <ul className="sr-calc-examples" aria-label={t('tools.percent.examples')}>
          {EXAMPLES.map((k) => (
            <li key={k}>
              <button type="button" className="sr-calc-hist-btn" onClick={() => setPhrase(k)}>
                <span dir="ltr">{k}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      <fieldset className="sr-calc-form">
        <legend>{t('tools.percent.form')}</legend>
        <label className="sr-calc-field">
          {t('tools.percent.kind')}
          <select value={kind} onChange={(e) => setKind(e.target.value as PercentKind)}>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {t(KIND_LABELS[k])}
              </option>
            ))}
          </select>
        </label>
        <div className="sr-calc-convert">
          <label className="sr-calc-field">
            {t(FIELD_A[kind])}
            <input
              className="sr-calc-input"
              type="text"
              inputMode="decimal"
              autoComplete="off"
              spellCheck={false}
              dir="ltr"
              value={aText}
              onChange={(e) => setAText(e.target.value)}
            />
          </label>
          <label className="sr-calc-field">
            {t(FIELD_B[kind])}
            <input
              className="sr-calc-input"
              type="text"
              inputMode="decimal"
              autoComplete="off"
              spellCheck={false}
              dir="ltr"
              value={bText}
              onChange={(e) => setBText(e.target.value)}
            />
          </label>
        </div>
        {show(fromForm, 'form')}
      </fieldset>
    </div>
  );
}
