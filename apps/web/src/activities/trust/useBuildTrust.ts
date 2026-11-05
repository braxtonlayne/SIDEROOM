// The trust badge of one game, loaded when it is shown, with the vouch actions. Plain state like the other
// Activity hooks (the Lounge has no query cache).
import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { httpTrustApi, type TrustApi } from './api';
import type { TrustView } from './types';

export interface BuildTrustState {
  trust: TrustView | null;
  failed: boolean;
  /** A vouch or withdrawal is in flight. */
  busy: boolean;
  /** The last vouch was refused (not in good standing, the game updated, too many today). */
  actionFailed: boolean;
  vouch(): Promise<void>;
  unvouch(): Promise<void>;
  reload(): void;
}

export function useBuildTrust(
  applicationId: string | undefined,
  api: TrustApi = httpTrustApi,
): BuildTrustState {
  const [trust, setTrust] = useState<TrustView | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    setTrust(null);
    setFailed(false);
    setActionFailed(false);
    if (!applicationId) return;
    let dead = false;
    api
      .get(applicationId)
      .then((t) => !dead && setTrust(t))
      // A 404 is a game with no trust record yet (featured before its first reviewed build): no badge, not an error
      // (D-479). Anything else is a failure the launcher and Admin report.
      .catch(
        (e: unknown) => !dead && !(e instanceof ApiError && e.status === 404) && setFailed(true),
      );
    return () => {
      dead = true;
    };
  }, [applicationId, api, tick]);

  const run = useCallback(async (act: () => Promise<TrustView>) => {
    setBusy(true);
    setActionFailed(false);
    try {
      setTrust(await act());
    } catch {
      // A refused vouch usually means the game shipped new code while the page was open: show the new badge.
      setActionFailed(true);
      setTick((n) => n + 1);
    } finally {
      setBusy(false);
    }
  }, []);

  const vouch = useCallback(async () => {
    if (!applicationId || !trust?.code_sha256) return;
    const hash = trust.code_sha256;
    await run(() => api.vouch(applicationId, hash));
  }, [api, applicationId, run, trust?.code_sha256]);

  const unvouch = useCallback(async () => {
    if (!applicationId) return;
    await run(() => api.unvouch(applicationId));
  }, [api, applicationId, run]);

  return {
    trust,
    failed,
    busy,
    actionFailed,
    vouch,
    unvouch,
    reload: () => setTick((n) => n + 1),
  };
}
