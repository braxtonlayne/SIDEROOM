// A live party card in a channel (WP-FE-010, WP-BE-008): what is playing, how many are watching, Join. It updates
// from the gateway while the channel is open.
import { Button, Icon } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { useT } from '../i18n';
import { useChannelParty } from './useChannelParty';
import './watchparty.css';

export function WatchPartyStrip({ channelId }: { channelId: string | undefined }) {
  const t = useT();
  const party = useChannelParty(channelId);
  const router = useRouter();
  if (!party) return null;
  const m = party.media;
  const n = party.participants.length;
  const title = m?.title ?? t('watchparty.empty.stripTitle');
  return (
    <aside className="wp-strip" aria-label={t('watchparty.strip.label')}>
      <Icon name="film" size={20} />
      <span className="wp-strip-text">
        <strong>{title}</strong>
        {m?.episode_label ? <span className="wp-muted"> · {m.episode_label}</span> : null}
        <span className="wp-muted">
          {' '}
          ·{' '}
          {t('watchparty.strip.status', {
            state: party.playback.playing
              ? t('watchparty.strip.playing')
              : t('watchparty.strip.paused'),
            n,
          })}
        </span>
      </span>
      <Button
        size="sm"
        onClick={() => router.history.push(`/cinema/party/${party.id}`)}
        aria-label={t('watchparty.strip.joinLabel', { title })}
      >
        {t('watchparty.strip.join')}
      </Button>
    </aside>
  );
}
