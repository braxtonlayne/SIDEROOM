// Small shared pieces for the Cinema publisher screens: a loader that keeps the HTTP status (so a 403 reads as a
// permission problem, a 404 as "not available yet"), list-field helpers and a mutation runner.
import { Button, EmptyState, Notice, TextInput } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../../api/client';
import { t, useT } from '../../i18n';
import { errorText } from '../ui';

export interface Loaded<T> {
  data: T | undefined;
  error: string | null;
  /** HTTP status of the failure, when there was one. */
  status: number | null;
  loading: boolean;
  reload: () => Promise<void>;
  set: (next: T | ((cur: T | undefined) => T)) => void;
}

export function useLoad<T>(key: string, fetcher: () => Promise<T>): Loaded<T> {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const fetchRef = useRef(fetcher);
  fetchRef.current = fetcher;
  const keyRef = useRef(key);
  keyRef.current = key;
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    const forKey = keyRef.current;
    try {
      const next = await fetchRef.current();
      if (!alive.current || keyRef.current !== forKey) return;
      setData(next);
      setError(null);
      setStatus(null);
    } catch (e) {
      if (!alive.current || keyRef.current !== forKey) return;
      setError(errorText(e, t('admin.cinemaHooks.loadError')));
      setStatus(e instanceof ApiError ? e.status : null);
    } finally {
      if (alive.current && keyRef.current === forKey) setLoading(false);
    }
  }, []);

  useEffect(() => {
    setData(undefined);
    setError(null);
    setStatus(null);
    setLoading(true);
    void reload();
  }, [key, reload]);

  return {
    data,
    error,
    status,
    loading,
    reload,
    set: (n) => setData((c) => (typeof n === 'function' ? (n as (c: T | undefined) => T)(c) : n)),
  };
}

/** Render a failed load: 403 is a clear permission state, anything else a retryable alert. */
export function LoadProblem({
  error,
  status,
  onRetry,
  what,
}: {
  error: string;
  status: number | null;
  onRetry?: () => void;
  what?: string;
}) {
  const t = useT();
  if (status === 403)
    return (
      <EmptyState
        title={t('admin.cinemaHooks.noAccess.title')}
        description={t('admin.cinemaHooks.noAccess.description', {
          what: what ?? t('admin.cinemaHooks.this'),
        })}
      />
    );
  return (
    <Notice variant="danger" role="alert" className="ss-status">
      {error}{' '}
      {onRetry && (
        <Button size="sm" variant="secondary" onClick={onRetry}>
          {t('admin.cinemaHooks.retry')}
        </Button>
      )}
    </Notice>
  );
}

/** Run an async action with busy and error state; the error is shown by `<Banner>`. */
export function useRun() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const run = useCallback(
    async <T,>(fn: () => Promise<T>, done?: string): Promise<T | undefined> => {
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const r = await fn();
        if (done) setNotice(done);
        return r;
      } catch (e) {
        setError(errorText(e));
        return undefined;
      } finally {
        setBusy(false);
      }
    },
    [],
  );
  return {
    busy,
    error,
    notice,
    run,
    flash: setNotice,
    clear: () => {
      setError(null);
      setNotice(null);
    },
  };
}

export function Banner({ error, notice }: { error?: string | null; notice?: string | null }) {
  return (
    <>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {error}
        </Notice>
      )}
    </>
  );
}

// ---- list fields ----

export const toList = (s: string): string[] =>
  s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);
export const fromList = (l: readonly string[] | undefined): string => (l ?? []).join(', ');

export function ListInput({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  hint?: string;
}) {
  const t = useT();
  return (
    <TextInput
      label={label}
      hint={hint ?? t('admin.cinemaHooks.listHint')}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

export const intOrNull = (s: string): number | null => {
  const t = s.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? Math.trunc(n) : null;
};

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="ss-card" aria-label={title}>
      <h2 className="ss-sub">{title}</h2>
      {children}
    </section>
  );
}

export const fmtClock = (ms: number): string => {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = String(m).padStart(h ? 2 : 1, '0');
  const ss = String(sec).padStart(2, '0');
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
};
