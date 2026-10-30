import { useToast } from '@sideroom/ui';
import { useCallback } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import {
  claimSeat,
  fetchIcs,
  getCard,
  partyInChannel,
  saveBlob,
  startParty,
  unclaimSeat,
  type InviteCard,
} from './api';

/** The four buttons of a card (`card:<action>:<id>`, the prefix of utl-cards; utl-collab uses `tool_room:`). */
export type CardAction = 'claim' | 'unclaim' | 'ics' | 'party';
export interface CardButton {
  action: CardAction;
  cardId: string;
}

const ID = /^card:(claim|unclaim|ics|party):(\d{1,20})$/;

/** `null` for any custom id that is not a card button. */
export function parseCardButton(customId: string | undefined): CardButton | null {
  const m = customId ? ID.exec(customId) : null;
  return m ? { action: m[1] as CardAction, cardId: m[2]! } : null;
}

export const isCardButton = (customId: string | undefined): boolean =>
  parseCardButton(customId) !== null;

/**
 * What pressing a card button does. A press is an ordinary request to the card routes, so the server decides who
 * may do what; the toast says what happened. Returns a function `(customId, channelId) => Promise<void>` that never
 * throws.
 */
export function useCardPress() {
  const t = useT();
  const toast = useToast();

  return useCallback(
    async (customId: string | undefined, channelId: string): Promise<void> => {
      const b = parseCardButton(customId);
      if (!b) return;
      const fail = (e: unknown, fallbackKey: Parameters<typeof t>[0]) =>
        toast.show({
          title: e instanceof ApiError && e.body?.message ? e.body.message : t(fallbackKey),
          variant: 'danger',
        });
      try {
        switch (b.action) {
          case 'claim': {
            const card = await claimSeat(channelId, b.cardId, true);
            const mine = card.mine;
            if (!mine) return;
            if (mine.reminder_deliverable) {
              toast.show({
                title: t('tools.cards.claimed'),
                description: (
                  <span>
                    {t('tools.cards.remindersOn')}{' '}
                    <button
                      type="button"
                      className="sr-card-link"
                      onClick={() =>
                        void claimSeat(channelId, b.cardId, false)
                          .then(() => toast.show({ title: t('tools.cards.remindersOff') }))
                          .catch((e: unknown) => fail(e, 'tools.cards.errorClaim'))
                      }
                    >
                      {t('tools.cards.turnOffReminders')}
                    </button>
                  </span>
                ),
                variant: 'success',
              });
            } else {
              toast.show({
                title: t('tools.cards.claimed'),
                description: t('tools.cards.remindersBlocked'),
                variant: 'warning',
                duration: 8000,
              });
            }
            return;
          }
          case 'unclaim': {
            await unclaimSeat(channelId, b.cardId);
            toast.show({ title: t('tools.cards.released'), variant: 'success' });
            return;
          }
          case 'ics': {
            const blob = await fetchIcs(channelId, b.cardId);
            saveBlob(blob, `invite-${b.cardId}.ics`);
            return;
          }
          case 'party': {
            const card: InviteCard = await getCard(channelId, b.cardId);
            if (!card.title_id) return;
            try {
              const party = await startParty(channelId, card.title_id);
              window.location.assign(`/cinema/party/${party.id}`);
            } catch (e) {
              if (e instanceof ApiError && e.status === 409) {
                // One is already running here: join it instead.
                const running = await partyInChannel(channelId);
                window.location.assign(`/cinema/party/${running.id}`);
              } else if (e instanceof ApiError && e.status === 403 && e.body?.code !== 900011) {
                toast.show({ title: t('tools.cards.partyDenied'), variant: 'warning' });
              } else {
                throw e;
              }
            }
            return;
          }
        }
      } catch (e) {
        if (e instanceof ApiError && e.status === 409) {
          toast.show({
            title: e.body?.message ?? t('tools.cards.full'),
            variant: 'warning',
          });
          return;
        }
        fail(e, b.action === 'party' ? 'tools.cards.errorParty' : 'tools.cards.errorClaim');
      }
    },
    [t, toast],
  );
}
