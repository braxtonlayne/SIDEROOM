import { useState, type ReactNode } from 'react';
import { useT } from '../../i18n';
import '../utl-dev.css';

/** Copy text to the clipboard and say so; falls back quietly when the clipboard is not available. */
export function CopyButton({ text, disabled }: { text: string; disabled?: boolean }) {
  const t = useT();
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle');
  return (
    <button
      type="button"
      className="ud-btn"
      disabled={disabled || text === ''}
      onClick={() => {
        void (async () => {
          try {
            await navigator.clipboard.writeText(text);
            setState('copied');
          } catch {
            setState('failed');
          }
          setTimeout(() => setState('idle'), 1800);
        })();
      }}
    >
      {state === 'copied'
        ? t('tools.shared.copied')
        : state === 'failed'
          ? t('tools.shared.copyFailed')
          : t('tools.shared.copy')}
    </button>
  );
}

export function Field({
  label,
  children,
  wide,
}: {
  label: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <label className="ud-field" style={wide ? { flexBasis: '100%' } : undefined}>
      <span>{label}</span>
      {children}
    </label>
  );
}

export function Problem({ children }: { children: ReactNode }) {
  return (
    <p role="alert" className="ud-error">
      {children}
    </p>
  );
}

/** The wrapper every tool of this agent renders inside. */
export function Root({ children, label }: { children: ReactNode; label?: string }) {
  return (
    <div className="ud-root" role="group" aria-label={label}>
      {children}
    </div>
  );
}
