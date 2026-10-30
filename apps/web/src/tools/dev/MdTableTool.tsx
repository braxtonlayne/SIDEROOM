import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { type Align, markdownTable, parseDelimited } from './logic';
import { CopyButton, Field, Root } from './parts';

/** Markdown table builder: paste a spreadsheet selection or CSV, pick alignments, copy the table. */
export default function MdTableTool(_props: ToolProps) {
  const t = useT();
  const [text, setText] = useState('');
  const [aligns, setAligns] = useState<Align[]>([]);
  const [pretty, setPretty] = useState(true);

  const rows = useMemo(() => parseDelimited(text), [text]);
  const cols = rows.length ? Math.max(...rows.map((r) => r.length)) : 0;
  const table = useMemo(() => markdownTable(rows, aligns, pretty), [rows, aligns, pretty]);

  return (
    <Root label={t('tools.md-table.name')}>
      <Field label={t('tools.mdtable.input')}>
        <textarea className="ud-mono" value={text} spellCheck={false} onChange={(e) => setText(e.target.value)} />
      </Field>
      <p className="ud-muted">{t('tools.mdtable.hint')}</p>
      <div className="ud-row">
        {Array.from({ length: cols }, (_, i) => (
          <Field key={i} label={t('tools.mdtable.column', { n: i + 1 })}>
            <select
              value={aligns[i] ?? 'none'}
              onChange={(e) =>
                setAligns((cur) => {
                  const next = [...cur];
                  next[i] = e.target.value as Align;
                  return next;
                })
              }
            >
              <option value="none">{t('tools.mdtable.default')}</option>
              <option value="left">{t('tools.mdtable.left')}</option>
              <option value="center">{t('tools.mdtable.center')}</option>
              <option value="right">{t('tools.mdtable.right')}</option>
            </select>
          </Field>
        ))}
        <label className="ud-row">
          <input type="checkbox" checked={pretty} onChange={(e) => setPretty(e.target.checked)} />
          <span>{t('tools.mdtable.pretty')}</span>
        </label>
      </div>
      <pre className="ud-out ud-mono" aria-label={t('tools.mdtable.output')}>
        {table}
      </pre>
      <div className="ud-row">
        <CopyButton text={table} />
      </div>
    </Root>
  );
}
