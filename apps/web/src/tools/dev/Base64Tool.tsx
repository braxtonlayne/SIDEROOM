import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { base64Decode, base64Encode } from './logic';
import { CopyButton, Field, Problem, Root } from './parts';

/** Base64 encode and decode (UTF-8 text; standard or URL-safe). */
export default function Base64Tool(_props: ToolProps) {
  const t = useT();
  const [mode, setMode] = useState<'encode' | 'decode'>('encode');
  const [urlSafe, setUrlSafe] = useState(false);
  const [text, setText] = useState('');

  const out = useMemo(() => {
    if (text === '') return { value: '', bad: false };
    if (mode === 'encode') return { value: base64Encode(text, urlSafe), bad: false };
    const v = base64Decode(text);
    return { value: v ?? '', bad: v === null };
  }, [mode, urlSafe, text]);

  return (
    <Root label={t('tools.base64.name')}>
      <div className="ud-row" role="group" aria-label={t('tools.base64.direction')}>
        <button type="button" className="ud-btn" aria-pressed={mode === 'encode'} onClick={() => setMode('encode')}>
          {t('tools.base64.encode')}
        </button>
        <button type="button" className="ud-btn" aria-pressed={mode === 'decode'} onClick={() => setMode('decode')}>
          {t('tools.base64.decode')}
        </button>
        <label className="ud-row">
          <input type="checkbox" checked={urlSafe} onChange={(e) => setUrlSafe(e.target.checked)} />
          <span>{t('tools.base64.urlSafe')}</span>
        </label>
      </div>
      <Field label={t('tools.base64.input')}>
        <textarea className="ud-mono" value={text} spellCheck={false} onChange={(e) => setText(e.target.value)} />
      </Field>
      {out.bad ? <Problem>{t('tools.base64.invalid')}</Problem> : null}
      <pre className="ud-out ud-mono" aria-label={t('tools.base64.output')}>
        {out.value}
      </pre>
      <div className="ud-row">
        <CopyButton text={out.value} />
      </div>
    </Root>
  );
}
