// Buttons on system-user cards: appointment offers and tester invitations in a DM, and the cards in the Staff
// Server (ADM-STF-004, ADM-STF-007, G-01). Their `custom_id` is `sr:<action>:<id>`; the server runs them, there is
// no app behind them.
import { apiFetch } from '../../api/client';
import { withReauth } from '../../admin/reauth';

export type StaffButton =
  | { kind: 'offer'; offerId: string; accept: boolean }
  | { kind: 'tester'; inviteId: string; accept: boolean }
  | { kind: 'card'; customId: string };

/** Offer buttons answer the offer; every other `sr:` id is a staff card action. */
export function parseStaffButton(customId: string | undefined): StaffButton | null {
  if (!customId?.startsWith('sr:')) return null;
  const m = /^sr:offer-(accept|decline):(\d+)$/.exec(customId);
  if (m) return { kind: 'offer', offerId: m[2]!, accept: m[1] === 'accept' };
  const tester = /^sr:tester-(accept|decline):(\d+)$/.exec(customId);
  if (tester) return { kind: 'tester', inviteId: tester[2]!, accept: tester[1] === 'accept' };
  return { kind: 'card', customId };
}

export interface StaffActionResult {
  /** Shown to the person under the card; null when the server sent no text. */
  message: string | null;
}

export async function runStaffButton(
  b: StaffButton,
  where: { channelId: string; messageId: string },
): Promise<StaffActionResult> {
  if (b.kind === 'tester') {
    await apiFetch<unknown>(`/developers/tester-invites/${b.inviteId}/respond`, {
      method: 'POST',
      retries: 0,
      json: { accept: b.accept },
    });
    return { message: null };
  }
  if (b.kind === 'offer') {
    await apiFetch<unknown>(`/staff/offers/${b.offerId}/respond`, {
      method: 'POST',
      retries: 0,
      json: { accept: b.accept },
    });
    return { message: null };
  }
  const out = await withReauth(() =>
    apiFetch<{ ok: boolean; message: string }>('/admin/card-actions', {
      method: 'POST',
      retries: 0,
      json: { channel_id: where.channelId, message_id: where.messageId, custom_id: b.customId },
    }),
  );
  return { message: out?.message ?? null };
}
