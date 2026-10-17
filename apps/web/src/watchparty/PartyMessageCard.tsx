// The party card inside the chat (WP-FE-010, WP-FE-008, WP-BE-008): the system user posts it as a message of type 200
// when a party starts in a channel and rewrites it when the media, the host or the headcount changes and when the
// party ends. The embed carries a text snapshot (what other clients show); while the party is live, this card draws
// from the shared gateway-driven party state instead, so it never lags. After the party ended it shows the snapshot.
import { Button, Icon } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import type { Message } from '../lounge/data';
import { useT } from '../i18n';
import { useChannelParty } from './useChannelParty';
import './watchparty.css';

/** Message type of the live party card (SIDE ROOM types start at 200). */
export const SYSTEM_WATCH_PARTY = 200;

/** The party id the card names: the last digits of its `/cinema/party/<id>` link. */
export function partyIdOf(url: string | null | undefined): string | null {
  const m = /\/cinema\/party\/(\d+)/.exec(url ?? '');
  return m?.[1] ?? null;
}

export function PartyMessageCard({ message }: { message: Pick<Message, 'channelId' | 'embeds'> }) {
  const t = useT();
  const router = useRouter();
  const embed = message.embeds?.[0];
  const id = partyIdOf(embed?.url);
  const channelParty = useChannelParty(message.channelId);
  const live = id !== null && channelParty?.id === id ? channelParty : null;

  const title = live?.media?.title ?? embed?.title ?? t('watchparty.empty.stripTitle');
  const episode = live ? live.media?.episode_label : null;
  const n = live?.participants.length ?? 0;

  return (
    <aside
      className="wp-card"
      data-live={live ? 'true' : 'false'}
      aria-label={t('watchparty.card.label', { title })}
    >
      <Icon name="film" size={20} />
      <span className="wp-strip-text">
        <strong>{title}</strong>
        {live ? (
          <>
            {episode ? <span className="wp-muted"> · {episode}</span> : null}
            <span className="wp-muted">
              {' '}
              ·{' '}
              {t('watchparty.strip.status', {
                state: live.playback.playing
                  ? t('watchparty.strip.playing')
                  : t('watchparty.strip.paused'),
                n,
              })}
            </span>
          </>
        ) : (
          <span className="wp-muted"> · {embed?.description ?? t('watchparty.card.ended')}</span>
        )}
      </span>
      {live ? (
        <Button
          size="sm"
          onClick={() => router.history.push(`/cinema/party/${live.id}`)}
          aria-label={t('watchparty.strip.joinLabel', { title })}
        >
          {t('watchparty.strip.join')}
        </Button>
      ) : (
        <span className="wp-muted">{t('watchparty.card.endedBadge')}</span>
      )}
    </aside>
  );
}
