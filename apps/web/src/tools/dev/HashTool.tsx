import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { CopyButton, Field, Problem, Root } from './parts';
import { computeHashes } from './hashes';

/** Hashes of text or of a file: SHA-1/256/384/512 through SubtleCrypto, MD5 and CRC-32 from our wasm. */
export default function HashTool() {
  const t = useT();
  const [text, setText] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [out, setOut] = useState<[string, string][]>([]);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const bytes = file
          ? new Uint8Array(await file.arrayBuffer())
          : new TextEncoder().encode(text);
        const res = await computeHashes(bytes);
        if (live) {
          setOut(res);
          setFailed(false);
        }
      } catch {
        if (live) setFailed(true);
      }
    })();
    return () => {
      live = false;
    };
  }, [text, file]);

  return (
    <Root label={t('tools.hash.name')}>
      <Field label={t('tools.hash.input')}>
        <textarea
          className="ud-mono"
          value={text}
          spellCheck={false}
          disabled={file !== null}
          onChange={(e) => setText(e.target.value)}
        />
      </Field>
      <div className="ud-row">
        <Field label={t('tools.hash.file')}>
          <input type="file" onChange={(e) => setFile(e.target.files?.[0] ?? null)} />
        </Field>
        {file ? (
          <button type="button" className="ud-btn" onClick={() => setFile(null)}>
            {t('tools.hash.clearFile')}
          </button>
        ) : null}
      </div>
      <p className="ud-warn">{t('tools.hash.note')}</p>
      {failed ? <Problem>{t('tools.hash.failed')}</Problem> : null}
      <dl className="ud-out ud-mono">
        {out.map(([name, value]) => (
          <div key={name}>
            <dt>{name}</dt>
            <dd style={{ margin: 0 }}>
              {value} <CopyButton text={value} />
            </dd>
          </div>
        ))}
      </dl>
    </Root>
  );
}
