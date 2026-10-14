import { Button, Notice, Spinner } from '@sideroom/ui';
import { useCallback, useRef, useState, type ReactNode } from 'react';
import { useT } from '../../i18n';
import { errorText } from './api';
import type { Resource } from './hooks';
import './server-settings.css';

export function SectionHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="ss-header">
      <div>
        <h1 className="ss-title">{title}</h1>
        {description && <p className="ss-desc">{description}</p>}
      </div>
      {actions && <div className="ss-actions">{actions}</div>}
    </header>
  );
}

export function Loading({ label }: { label?: string }) {
  const t = useT();
  return (
    <div className="ss-loading">
      <Spinner label={label ?? t('settings.ui.loading')} />
    </div>
  );
}

export function LoadError({ message, onRetry }: { message: string; onRetry?: () => void }) {
  const t = useT();
  return (
    <Notice variant="danger" title={t('settings.ui.loadErrorTitle')}>
      <p className="ss-notice-text">{message}</p>
      {onRetry && (
        <Button size="sm" variant="secondary" onClick={onRetry}>
          {t('settings.ui.tryAgain')}
        </Button>
      )}
    </Notice>
  );
}

/** Loading / error / content for a fetched resource. */
export function Async<T>({
  res,
  children,
  label,
}: {
  res: Resource<T>;
  children: (data: T) => ReactNode;
  label?: string;
}) {
  if (res.data !== undefined) return <>{children(res.data)}</>;
  if (res.error) return <LoadError message={res.error} onRetry={() => void res.reload()} />;
  return <Loading {...(label ? { label } : {})} />;
}

/** Runs a mutation, tracking busy state and a user-facing error. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const live = useRef(true);
  const run = useCallback(
    async <T,>(fn: () => Promise<T>, doneText?: string): Promise<T | undefined> => {
      setBusy(true);
      setError(null);
      setDone(null);
      try {
        const out = await fn();
        if (doneText && live.current) setDone(doneText);
        return out;
      } catch (e) {
        if (live.current) setError(errorText(e));
        return undefined;
      } finally {
        if (live.current) setBusy(false);
      }
    },
    [],
  );
  const clear = useCallback(() => {
    setError(null);
    setDone(null);
  }, []);
  return { busy, error, done, run, clear, setError };
}

export function ActionStatus({ error, done }: { error: string | null; done?: string | null }) {
  if (error)
    return (
      <Notice variant="danger" className="ss-status">
        {error}
      </Notice>
    );
  if (done)
    return (
      <Notice variant="success" className="ss-status">
        {done}
      </Notice>
    );
  return null;
}

/** Small colored dot for role colors. */
export function ColorDot({ color }: { color: number }) {
  return (
    <span
      className="ss-dot"
      aria-hidden="true"
      style={{
        background: color ? `#${color.toString(16).padStart(6, '0')}` : 'var(--ui-text-faint)',
      }}
    />
  );
}

export const formatDate = (iso: string | null | undefined) => {
  if (!iso) return '';
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ''
    : d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
};

export const formatDateTime = (iso: string | null | undefined) => {
  if (!iso) return '';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleString();
};

export function NotAvailable({ children }: { children: ReactNode }) {
  const t = useT();
  return (
    <p className="ss-na">
      <em>{t('settings.ui.notAvailable')}</em> {children}
    </p>
  );
}
