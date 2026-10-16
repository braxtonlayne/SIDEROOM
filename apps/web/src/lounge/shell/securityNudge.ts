/**
 * Second-factor notice for older accounts (H-21, P13-WFE-1). The welcome dialog greets accounts younger than
 * 14 days; every other account without TOTP and without a passkey gets one dismissable line in the notices bar.
 * Dismissal is the synced settings key `securityNudgeDone` (`PATCH /users/@me/settings`).
 *
 * Conservative on every unknown: the line shows only when the profile says `mfa_enabled === false`, the passkey
 * list loaded empty and the settings blob loaded without the flag.
 */
import { useCallback, useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { listPasskeys } from '../../api/passkeys';

/**
 * Platform snowflake epoch (docs/05 section 3: `SIDEROOM_EPOCH_MS`, 2026-01-01T00:00:00Z). Temporary single
 * TypeScript copy until the generated constant from the Rust snowflake crate lands (P13-MFA-5, H-26).
 */
export const SIDEROOM_EPOCH_MS = 1767225600000n;
export const WELCOME_WINDOW_MS = 14 * 24 * 60 * 60 * 1000;
export const SECURITY_NUDGE_KEY = 'securityNudgeDone';

/** True when the snowflake id is older than the welcome window (an unparsable id counts as older). */
export function accountOlderThanWelcome(id: string | undefined, now = Date.now()): boolean {
  if (!id || !/^\d+$/.test(id)) return true;
  const created = Number((BigInt(id) >> 22n) + SIDEROOM_EPOCH_MS);
  return now - created >= WELCOME_WINDOW_MS;
}

interface SettingsResponse {
  settings: Record<string, unknown>;
}

/** `show` is true when the notice should be listed; `dismiss` hides it now and stores the flag. */
export function useSecurityNudge(): { show: boolean; dismiss: () => void } {
  const [eligible, setEligible] = useState(false);
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const me = await apiFetch<{ id?: string; mfa_enabled?: boolean }>('/users/@me', {
          retries: 0,
        });
        if (me.mfa_enabled !== false || !accountOlderThanWelcome(me.id)) return;
        const [keys, blob] = await Promise.all([
          listPasskeys(),
          apiFetch<SettingsResponse>('/users/@me/settings', { retries: 0 }),
        ]);
        if (!Array.isArray(keys) || keys.length > 0) return;
        if (!blob?.settings || blob.settings[SECURITY_NUDGE_KEY] === true) return;
        if (live) setEligible(true);
      } catch {
        // Unknown state: show nothing.
      }
    })();
    return () => {
      live = false;
    };
  }, []);

  const dismiss = useCallback(() => {
    setHidden(true);
    void apiFetch('/users/@me/settings', {
      method: 'PATCH',
      json: { settings: { [SECURITY_NUDGE_KEY]: true } },
      retries: 0,
    }).catch(() => undefined);
  }, []);

  return { show: eligible && !hidden, dismiss };
}
