import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { formatJson, parseJson } from './logic';
import { CopyButton, Field, Problem, Root } from './parts';

type Indent = '2' | '4' | 'tab' | 'min';

/** JSON: validate (with line and column), format, minify, sort keys. */
export default function JsonTool() {
  const t = useT();
  const [text, setText] = useState('');
  const [indent, setIndent] = useState<Indent>('2');
  const [sorted, setSorted] = useState(false);

  const result = useMemo(() => {
    if (text.trim() === '') return null;
    const i = indent === 'min' ? 0 : indent === 'tab' ? 'tab' : indent === '4' ? 4 : 2;
    return formatJson(text, i, sorted);
  }, [text, indent, sorted]);

  const problem = result && !result.ok ? result.problem : null;
  const valid = text.trim() !== '' && parseJson(text).ok;

  return (
    <Root label={t('tools.json.name')}>
      <Field label={t('tools.json.input')}>
        <textarea
          className="ud-mono"
          value={text}
          spellCheck={false}
          aria-invalid={problem !== null}
          onChange={(e) => setText(e.target.value)}
        />
      </Field>
      <div className="ud-row">
        <Field label={t('tools.json.indent')}>
          <select value={indent} onChange={(e) => setIndent(e.target.value as Indent)}>
            <option value="2">{t('tools.json.spaces', { n: 2 })}</option>
            <option value="4">{t('tools.json.spaces', { n: 4 })}</option>
            <option value="tab">{t('tools.json.tab')}</option>
            <option value="min">{t('tools.json.minify')}</option>
          </select>
        </Field>
        <label className="ud-field ud-field-narrow">
          <span>{t('tools.json.sortKeys')}</span>
          <input type="checkbox" checked={sorted} onChange={(e) => setSorted(e.target.checked)} />
        </label>
      </div>
      {problem ? (
        <Problem>
          {problem.line !== null
            ? t('tools.json.errorAt', {
                line: problem.line,
                column: problem.column ?? 0,
                message: problem.message,
              })
            : problem.message}
        </Problem>
      ) : valid ? (
        <p role="status" className="ud-muted">
          {t('tools.json.valid')}
        </p>
      ) : null}
      {result?.ok ? (
        <>
          <pre className="ud-out ud-mono" role="group" aria-label={t('tools.json.output')}>
            {result.text}
          </pre>
          <div className="ud-row">
            <CopyButton text={result.text} />
          </div>
        </>
      ) : null}
    </Root>
  );
}
