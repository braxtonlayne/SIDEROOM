import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { type UrlMode, urlDecode, urlEncode, urlParams } from './logic';
import { CopyButton, Field, Problem, Root } from './parts';

/** URL encode and decode (component or whole address), and a list of a URL's query parameters. */
export default function UrlTool(_props: ToolProps) {
  const t = useT();
  const [direction, setDirection] = useState<'encode' | 'decode'>('encode');
  const [kind, setKind] = useState<UrlMode>('component');
  const [text, setText] = useState('');

  const out = useMemo(() => {
    if (text === '') return { value: '', bad: false };
    if (direction === 'encode') return { value: urlEncode(text, kind), bad: false };
    const v = urlDecode(text, kind);
    return { value: v ?? '', bad: v === null };
  }, [direction, kind, text]);
  const params = useMemo(() => urlParams(text), [text]);

  return (
    <Root label={t('tools.url.name')}>
      <div className="ud-row" role="group" aria-label={t('tools.url.direction')}>
        <button type="button" className="ud-btn" aria-pressed={direction === 'encode'} onClick={() => setDirection('encode')}>
          {t('tools.url.encode')}
        </button>
        <button type="button" className="ud-btn" aria-pressed={direction === 'decode'} onClick={() => setDirection('decode')}>
          {t('tools.url.decode')}
        </button>
        <Field label={t('tools.url.scope')}>
          <select value={kind} onChange={(e) => setKind(e.target.value as UrlMode)}>
            <option value="component">{t('tools.url.component')}</option>
            <option value="full">{t('tools.url.full')}</option>
          </select>
        </Field>
      </div>
      <Field label={t('tools.url.input')}>
        <textarea className="ud-mono" value={text} spellCheck={false} onChange={(e) => setText(e.target.value)} />
      </Field>
      {out.bad ? <Problem>{t('tools.url.invalid')}</Problem> : null}
      <pre className="ud-out ud-mono" aria-label={t('tools.url.output')}>
        {out.value}
      </pre>
      <div className="ud-row">
        <CopyButton text={out.value} />
      </div>
      {params.length > 0 ? (
        <table className="ud-out ud-mono" aria-label={t('tools.url.params')}>
          <tbody>
            {params.map(([k, v], i) => (
              <tr key={i}>
                <th scope="row">{k}</th>
                <td>{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </Root>
  );
}
