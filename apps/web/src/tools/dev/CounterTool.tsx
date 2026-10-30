import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { countText } from './logic';
import { Field, Root } from './parts';

/** Character, word, line, sentence and byte counts, and a reading time. */
export default function CounterTool() {
  const t = useT();
  const [text, setText] = useState('');
  const c = useMemo(() => countText(text), [text]);
  const rows: [string, number][] = [
    [t('tools.counter.characters'), c.characters],
    [t('tools.counter.noSpaces'), c.charactersNoSpaces],
    [t('tools.counter.words'), c.words],
    [t('tools.counter.lines'), c.lines],
    [t('tools.counter.sentences'), c.sentences],
    [t('tools.counter.paragraphs'), c.paragraphs],
    [t('tools.counter.bytes'), c.bytes],
    [t('tools.counter.reading'), c.readingMinutes],
  ];
  return (
    <Root label={t('tools.counter.name')}>
      <Field label={t('tools.counter.input')}>
        <textarea value={text} onChange={(e) => setText(e.target.value)} />
      </Field>
      <dl className="ud-out" aria-live="polite">
        {rows.map(([label, n]) => (
          <div key={label} className="ud-row" style={{ justifyContent: 'space-between' }}>
            <dt>{label}</dt>
            <dd style={{ margin: 0 }}>{n}</dd>
          </div>
        ))}
      </dl>
    </Root>
  );
}
