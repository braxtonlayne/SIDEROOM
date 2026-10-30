import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { Field, Root } from '../dev/parts';
import { pickWeighted } from './engine';

/** True when the member (or the OS) asked for less motion: the wheel then lands without spinning. */
export function prefersReducedMotion(): boolean {
  try {
    if (document.documentElement.dataset.reducedMotion === 'true') return true;
    return (
      typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches
    );
  } catch {
    return false;
  }
}

const MAX_NAMES = 100;

/** One name per line in; a wheel picks one. The pick is made first, then the wheel turns to show it. */
export default function WheelTool() {
  const t = useT();
  const [text, setText] = useState('');
  const [winner, setWinner] = useState<string | null>(null);
  const [turn, setTurn] = useState(0);
  const names = useMemo(
    () =>
      Array.from(
        new Set(
          text
            .split('\n')
            .map((n) => n.trim())
            .filter(Boolean),
        ),
      ).slice(0, MAX_NAMES),
    [text],
  );

  const spin = async () => {
    if (names.length === 0) return;
    const i = await pickWeighted(names.map(() => 1));
    if (i < 0) return;
    setWinner(names[i] ?? null);
    // Land the middle of slice i under the pointer at the top: full turns plus the offset.
    const slice = 360 / names.length;
    const target = 360 - (i + 0.5) * slice;
    setTurn((cur) =>
      prefersReducedMotion() ? target : cur + 360 * 4 + ((target - (cur % 360) + 360) % 360),
    );
  };

  const gradient = useMemo(() => {
    if (names.length === 0) return 'var(--sr-bg-1, #14120f)';
    const slice = 100 / names.length;
    const shades = ['var(--sr-bg-2, #1c1914)', 'var(--sr-bg-1, #14120f)'];
    return `conic-gradient(${names
      .map((_, i) => `${shades[i % 2]} ${i * slice}% ${(i + 1) * slice}%`)
      .join(', ')})`;
  }, [names]);

  return (
    <Root label={t('tools.wheel.name')}>
      <Field label={t('tools.wheel.names')}>
        <textarea
          value={text}
          placeholder={t('tools.wheel.placeholder')}
          onChange={(e) => {
            setText(e.target.value);
            setWinner(null);
          }}
        />
      </Field>
      <div className="ud-wheel" aria-hidden="true">
        <div className="ud-wheel-pointer" />
        <div
          className="ud-wheel-face"
          style={{ background: gradient, transform: `rotate(${turn}deg)` }}
        />
      </div>
      <div className="ud-row" style={{ justifyContent: 'center' }}>
        <button
          type="button"
          className="ud-btn ud-btn-primary"
          disabled={names.length === 0}
          onClick={() => void spin()}
        >
          {t('tools.wheel.spin')}
        </button>
      </div>
      <p className="ud-muted" style={{ textAlign: 'center' }}>
        {t('tools.wheel.count', { n: names.length })}
      </p>
      <p className="ud-winner" role="status" aria-live="polite">
        {winner !== null ? t('tools.wheel.winner', { name: winner }) : ''}
      </p>
    </Root>
  );
}
