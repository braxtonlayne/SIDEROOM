// Shared "Are you 18 or older?" gate (D-032, D-184; docs/06 section 7). One dialog and one hook for every
// place that opens age-restricted content: the Library reader today, the Lounge channel interstitial
// (LNG-CHN-022) next. Mount <AgeGateProvider> once above the section, then:
//
//   const { ensureAdult } = useAgeGate();
//   if (!(await ensureAdult())) return;          // dialog shown at most once per session; "yes" is permanent
//
// or wrap a request: `withAgeGate(fetchManifest, ensureAdult)` retries once after a 403 with code 900011.
import { Button, Modal } from '@sideroom/ui';
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { ApiError, apiPost } from '../api/client';
import { useT } from '../i18n';

/** Error code the API answers with when an account has not yet confirmed it is 18 or older. */
export const AGE_CONFIRMATION_REQUIRED = 900011;

export const isAgeGateError = (e: unknown): e is ApiError =>
  e instanceof ApiError && e.status === 403 && e.body?.code === AGE_CONFIRMATION_REQUIRED;

/** `POST /sr/age-gate`: records the answer on the account. */
export const postAgeAnswer = (over18: boolean) =>
  apiPost<void>('/sr/age-gate', { over_18: over18 });

/**
 * Runs `fn`; when it fails with the age-confirmation error, asks `ensureAdult` and, on yes, runs `fn` once
 * more. On no the original error is rethrown so the caller can leave the page.
 */
export async function withAgeGate<T>(fn: () => Promise<T>, ensureAdult: () => Promise<boolean>) {
  try {
    return await fn();
  } catch (e) {
    if (!isAgeGateError(e)) throw e;
    if (!(await ensureAdult())) throw e;
    return fn();
  }
}

export interface AgeGateApi {
  /** Resolves true once the account has confirmed 18+, false when the user declines or dismisses. */
  ensureAdult: () => Promise<boolean>;
}

const AgeGateContext = createContext<AgeGateApi | null>(null);

export function useAgeGate(): AgeGateApi {
  const ctx = useContext(AgeGateContext);
  if (!ctx) throw new Error('useAgeGate needs an <AgeGateProvider>');
  return ctx;
}

/** Like [`useAgeGate`], for places that may be rendered outside an <AgeGateProvider> (the Lounge): null there. */
export function useOptionalAgeGate(): AgeGateApi | null {
  return useContext(AgeGateContext);
}

export function AgeGateProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const confirmed = useRef(false);
  const pending = useRef<{ promise: Promise<boolean>; resolve: (v: boolean) => void } | null>(null);

  const ensureAdult = useCallback((): Promise<boolean> => {
    if (confirmed.current) return Promise.resolve(true);
    if (pending.current) return pending.current.promise;
    let resolve!: (v: boolean) => void;
    const promise = new Promise<boolean>((r) => (resolve = r));
    pending.current = { promise, resolve };
    setError(null);
    setOpen(true);
    return promise;
  }, []);

  const settle = useCallback((answer: boolean) => {
    pending.current?.resolve(answer);
    pending.current = null;
    setOpen(false);
  }, []);

  const answer = async (over18: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await postAgeAnswer(over18);
      if (over18) confirmed.current = true;
      settle(over18);
    } catch {
      setError(t('lib.ageGate.saveFailed'));
    } finally {
      setBusy(false);
    }
  };

  const api = useMemo(() => ({ ensureAdult }), [ensureAdult]);
  return (
    <AgeGateContext.Provider value={api}>
      {children}
      <Modal
        open={open}
        onClose={() => settle(false)}
        title={t('lib.ageGate.title')}
        description={t('lib.ageGate.description')}
        size="sm"
        footer={
          <>
            <Button variant="secondary" disabled={busy} onClick={() => void answer(false)}>
              {t('lib.ageGate.no')}
            </Button>
            <Button disabled={busy} onClick={() => void answer(true)}>
              {t('lib.ageGate.yes')}
            </Button>
          </>
        }
      >
        {error && <p role="alert">{error}</p>}
      </Modal>
    </AgeGateContext.Provider>
  );
}
