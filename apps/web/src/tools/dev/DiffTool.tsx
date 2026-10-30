import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { canonicalJson } from './logic';
import { Field, Problem, Root } from './parts';
import { devWasm } from './wasmTools';

type Mode = 'lines' | 'words' | 'chars' | 'json';
interface Op {
  op: 'eq' | 'add' | 'del';
  text: string;
}
interface Diffed {
  ops: Op[];
  added: number;
  removed: number;
}

/** Text and JSON diff. The comparison is our LCS in `sr-tools-wasm`; JSON is compared by content (sorted keys). */
export default function DiffTool() {
  const t = useT();
  const [a, setA] = useState('');
  const [b, setB] = useState('');
  const [mode, setMode] = useState<Mode>('lines');
  const [res, setRes] = useState<Diffed | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      setProblem(null);
      if (a === '' && b === '') {
        setRes(null);
        return;
      }
      let left = a;
      let right = b;
      if (mode === 'json') {
        const ja = a.trim() === '' ? { ok: true as const, text: '' } : canonicalJson(a);
        const jb = b.trim() === '' ? { ok: true as const, text: '' } : canonicalJson(b);
        if (!ja.ok || !jb.ok) {
          if (live) {
            setRes(null);
            setProblem(t('tools.diff.badJson'));
          }
          return;
        }
        left = ja.text === '' ? '' : `${ja.text}\n`;
        right = jb.text === '' ? '' : `${jb.text}\n`;
      }
      try {
        const w = await devWasm();
        const r = JSON.parse(w.diffText(left, right, mode === 'json' ? 'lines' : mode)) as Diffed;
        if (live) setRes(r);
      } catch (e) {
        if (live) {
          setRes(null);
          setProblem(
            e instanceof Error && /too-large/.test(e.message)
              ? t('tools.diff.tooLarge')
              : t('tools.diff.failed'),
          );
        }
      }
    })();
    return () => {
      live = false;
    };
  }, [a, b, mode, t]);

  return (
    <Root label={t('tools.diff.name')}>
      <Field label={t('tools.diff.mode')}>
        <select value={mode} onChange={(e) => setMode(e.target.value as Mode)}>
          <option value="lines">{t('tools.diff.lines')}</option>
          <option value="words">{t('tools.diff.words')}</option>
          <option value="chars">{t('tools.diff.chars')}</option>
          <option value="json">{t('tools.diff.json')}</option>
        </select>
      </Field>
      <div className="ud-row">
        <Field label={t('tools.diff.before')}>
          <textarea
            className="ud-mono"
            value={a}
            spellCheck={false}
            onChange={(e) => setA(e.target.value)}
          />
        </Field>
        <Field label={t('tools.diff.after')}>
          <textarea
            className="ud-mono"
            value={b}
            spellCheck={false}
            onChange={(e) => setB(e.target.value)}
          />
        </Field>
      </div>
      {problem ? <Problem>{problem}</Problem> : null}
      {res ? (
        <>
          <p className="ud-muted" role="status">
            {t('tools.diff.summary', { added: res.added, removed: res.removed })}
          </p>
          <pre className="ud-out ud-mono" role="group" aria-label={t('tools.diff.output')}>
            {res.ops.map((o, i) =>
              o.op === 'eq' ? (
                <span key={i}>{o.text}</span>
              ) : o.op === 'add' ? (
                <ins key={i} className="ud-diff-add">
                  {o.text}
                </ins>
              ) : (
                <del key={i} className="ud-diff-del">
                  {o.text}
                </del>
              ),
            )}
          </pre>
        </>
      ) : null}
    </Root>
  );
}
