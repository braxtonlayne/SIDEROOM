import { useState } from 'react';
import { useT } from '../../i18n';
import { Field, Problem, Root } from '../dev/parts';
import type { ToolProps } from '../registry';
import { type RollErrorCode, type RollResult, rollDice } from './engine';

const QUICK = ['d4', 'd6', 'd8', 'd10', 'd12', 'd20', 'd%'] as const;
const MAX_HISTORY = 20;

/** Dice notation roller: `2d20kh1+5`, `4d6dl1`, `d%`, exploding `!`, success counting `>=`. */
export default function DiceTool(_props: ToolProps) {
  const t = useT();
  const [expr, setExpr] = useState('d20');
  const [result, setResult] = useState<RollResult | null>(null);
  const [error, setError] = useState<{ code: RollErrorCode; at?: number } | null>(null);
  const [history, setHistory] = useState<RollResult[]>([]);

  const roll = async (e: string) => {
    const out = await rollDice(e);
    if (out.ok) {
      setError(null);
      setResult(out.result);
      setHistory((h) => [out.result, ...h].slice(0, MAX_HISTORY));
    } else {
      setError(out);
    }
  };

  return (
    <Root label={t('tools.dice.name')}>
      <form
        className="ud-row"
        onSubmit={(e) => {
          e.preventDefault();
          void roll(expr);
        }}
      >
        <Field label={t('tools.dice.expression')}>
          <input
            className="ud-mono"
            value={expr}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={error !== null}
            onChange={(e) => setExpr(e.target.value)}
          />
        </Field>
        <button type="submit" className="ud-btn ud-btn-primary">
          {t('tools.dice.roll')}
        </button>
      </form>
      <div className="ud-row" role="group" aria-label={t('tools.dice.quick')}>
        {QUICK.map((q) => (
          <button
            key={q}
            type="button"
            className="ud-btn"
            onClick={() => {
              setExpr(q);
              void roll(q);
            }}
          >
            {q}
          </button>
        ))}
      </div>
      <p className="ud-muted">{t('tools.dice.syntax')}</p>
      {error ? (
        <Problem>
          {t(`tools.dice.error.${error.code}`)}
          {error.at !== undefined ? ` (${error.at + 1})` : ''}
        </Problem>
      ) : null}
      {result ? (
        <div role="status" aria-live="polite">
          <div className="ud-dice-total">{result.total}</div>
          {result.groups.map((g, i) => (
            <div key={i}>
              <span className="ud-muted">
                {g.notation} = {g.total}
              </span>
              <ul className="ud-dice-list">
                {g.dice.map((d, j) => (
                  <li
                    key={j}
                    className="ud-die"
                    data-dropped={d.dropped}
                    data-exploded={d.exploded}
                    data-success={d.success}
                  >
                    {d.value}
                    {d.dropped ? <span className="ud-sr"> {t('tools.dice.dropped')}</span> : null}
                    {d.exploded ? <span className="ud-sr"> {t('tools.dice.exploded')}</span> : null}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      ) : null}
      {history.length > 1 ? (
        <>
          <h4>{t('tools.dice.history')}</h4>
          <ol className="ud-history">
            {history.slice(1).map((h, i) => (
              <li key={i}>
                <code>{h.expression}</code> = {h.total}
              </li>
            ))}
          </ol>
        </>
      ) : null}
    </Root>
  );
}
