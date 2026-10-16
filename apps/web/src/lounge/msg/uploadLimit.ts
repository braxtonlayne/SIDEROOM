import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';

/** Below this size no upload limit is worth looking up. */
export const CHECK_FROM = 8 * 1024 * 1024;

export interface UploadLimits {
  /** Largest file the member can attach outside a server perk. */
  bytes: number;
  patron: boolean;
}

let cached: Promise<UploadLimits | null> | null = null;
export const resetUploadLimits = () => {
  cached = null;
};

/** `GET /donations/me`: the caller's effective upload size and whether they are a Patron. Read once. */
function loadLimits(): Promise<UploadLimits | null> {
  cached ??= apiFetch<{
    limits: { upload_bytes: number; patron: boolean };
    patron: { active: boolean };
  }>('/donations/me', { retries: 0 }).then(
    (me) => ({ bytes: me.limits.upload_bytes, patron: me.patron.active || me.limits.patron }),
    () => {
      cached = null;
      return null;
    },
  );
  return cached;
}

/** The member's upload limits, fetched only once a large file is in the tray (LNG-CMP-005). */
export function useUploadLimits(needed: boolean): UploadLimits | null {
  const [limits, setLimits] = useState<UploadLimits | null>(null);
  useEffect(() => {
    if (!needed) return;
    let live = true;
    void loadLimits().then((l) => live && setLimits(l));
    return () => {
      live = false;
    };
  }, [needed]);
  return limits;
}

/**
 * Whether a file is above the member's own limit. The server has the last word (server boosts can raise the
 * limit inside a server), so this only warns and never blocks.
 */
export const limitHint = (size: number, limits: UploadLimits | null): boolean =>
  !!limits && limits.bytes > 0 && size > limits.bytes;
