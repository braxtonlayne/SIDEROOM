import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { MAX_REGEX_INPUT, runRegex } from './logic';
import { Field, Problem, Root } from './parts';

const FLAGS = ['g', 'i', 'm', 's', 'u'] as const;

/** Regex tester over the browser's own RegExp, with the flavour note the roadmap asks for. */
export default function RegexTool() {
  const t = useT();
  const [pattern, setPattern] = useState('');
  const [flags, setFlags] = useState('g');
  const [text, setText] = useState('');

  const result = useMemo(
    () => (pattern === '' ? null : runRegex(pattern, flags, text)),
    [pattern, flags, text],
  );
  const toggle = (f: string) => setFlags((cur) => (cur.includes(f) ? cur.replace(f, '') : cur + f));

  return (
    <Root label={t('tools.regex.name')}>
      <p className="ud-warn">{t('tools.regex.flavour')}</p>
      <Field label={t('tools.regex.pattern')}>
        <input
          className="ud-mono"
          value={pattern}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={result !== null && !result.ok}
          onChange={(e) => setPattern(e.target.value)}
        />
      </Field>
      <div className="ud-row" role="group" aria-label={t('tools.regex.flags')}>
        {FLAGS.map((f) => (
          <button
            key={f}
            type="button"
            className="ud-btn"
            aria-pressed={flags.includes(f)}
            onClick={() => toggle(f)}
          >
            {f}
          </button>
        ))}
      </div>
      <Field label={t('tools.regex.text')}>
        <textarea
          value={text}
          maxLength={MAX_REGEX_INPUT}
          spellCheck={false}
          onChange={(e) => setText(e.target.value)}
        />
      </Field>
      {result && !result.ok ? <Problem>{result.message}</Problem> : null}
      {result?.ok ? (
        <div role="status">
          <p className="ud-muted">{t('tools.regex.count', { n: result.matches.length })}</p>
          {result.truncated ? <p className="ud-muted">{t('tools.regex.truncated')}</p> : null}
          <ol className="ud-history ud-mono">
            {result.matches.map((m, i) => (
              <li key={i}>
                <code>{m.text === '' ? t('tools.regex.empty') : m.text}</code>
                <span className="ud-muted"> @{m.index}</span>
                {m.groups.map((g, gi) => (
                  <span key={gi} className="ud-muted">
                    {' '}
                    ${gi + 1}={g === undefined ? '-' : JSON.stringify(g)}
                  </span>
                ))}
                {Object.entries(m.named).map(([k, v]) => (
                  <span key={k} className="ud-muted">
                    {' '}
                    {k}={v === undefined ? '-' : JSON.stringify(v)}
                  </span>
                ))}
              </li>
            ))}
          </ol>
        </div>
      ) : null}
    </Root>
  );
}
