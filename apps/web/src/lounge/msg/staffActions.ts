// Buttons on system-user cards: appointment offers and tester invitations in a DM, and the cards in the Staff
// Server (ADM-STF-004, ADM-STF-007, G-01). Their `custom_id` is `sr:<action>:<id>`; the server runs them, there is
// no app behind them.
import { apiFetch } from '../../api/client';
import { withReauth } from '../../admin/reauth';

export type StaffButton =
  | { kind: 'offer'; offerId: string; accept: boolean }
  | { kind: 'tester'; inviteId: string; accept: boolean }
  /** `#review-queue` card (G-68): approve runs on the server, reject opens the review page's reject dialog. */
  | { kind: 'review'; submissionId: string; approve: boolean; customId: string }
  | { kind: 'card'; customId: string };

/** Offer buttons answer the offer; every other `sr:` id is a staff card action. */
export function parseStaffButton(customId: string | undefined): StaffButton | null {
  if (!customId?.startsWith('sr:')) return null;
  const m = /^sr:offer-(accept|decline):(\d+)$/.exec(customId);
  if (m) return { kind: 'offer', offerId: m[2]!, accept: m[1] === 'accept' };
  const tester = /^sr:tester-(accept|decline):(\d+)$/.exec(customId);
  if (tester) return { kind: 'tester', inviteId: tester[2]!, accept: tester[1] === 'accept' };
  const review = /^sr:review-(approve|reject):(\d+)$/.exec(customId);
  if (review)
    return { kind: 'review', submissionId: review[2]!, approve: review[1] === 'approve', customId };
  return { kind: 'card', customId };
}

export interface StaffActionResult {
  /** Shown to the person under the card; null when the server sent no text. */
  message: string | null;
  /** Where the card sends the person next (an app path), for example the review page of a rejection. */
  link?: string | null;
  /** False when the server refused politely ("already decided"); the card stays as it is. */
  ok?: boolean;
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
    apiFetch<{ ok: boolean; message: string; link?: string | null }>('/admin/card-actions', {
      method: 'POST',
      retries: 0,
      json: { channel_id: where.channelId, message_id: where.messageId, custom_id: b.customId },
    }),
  );
  return { message: out?.message ?? null, link: out?.link ?? null, ok: out?.ok ?? true };
}
