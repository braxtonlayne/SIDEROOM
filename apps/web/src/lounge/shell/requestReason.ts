// Why a conversation sits in Message Requests because of a server's Protection settings (G-95, D-224).
import type { DmObject } from './api';

/** `SIDEROOM_EPOCH_MS` of crates/sr-core/src/snowflake.rs (2026-01-01); ids count milliseconds from it. */
const SIDEROOM_EPOCH_MS = 1_767_225_600_000n;

/** Whole days since the account with this snowflake id was created; null when the id is not a snowflake. */
export function accountAgeDays(id: string | undefined, nowMs: number = Date.now()): number | null {
  if (!id || !/^\d+$/.test(id)) return null;
  const created = Number((BigInt(id) >> 22n) + SIDEROOM_EPOCH_MS);
  return Math.max(0, Math.floor((nowMs - created) / 86_400_000));
}

/** The line shown on a request card, or null when no server setting sent it there. */
export function requestReasonText(
  dm: Pick<DmObject, 'sr_request_reason' | 'recipients'>,
  t: (key: string, vars?: Record<string, string | number>) => string,
  nowMs?: number,
): string | null {
  if (dm.sr_request_reason === 'new_account') {
    const days = accountAgeDays(dm.recipients[0]?.id, nowMs);
    return days === null ? null : t('lounge.requests.reasonNewAccount', { days });
  }
  if (dm.sr_request_reason === 'server_filter') return t('lounge.requests.reasonServerFilter');
  return null;
}
