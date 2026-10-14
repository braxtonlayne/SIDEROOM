// A live party card in a channel (WP-FE-010, WP-BE-008): what is playing, how many are watching, Join. It updates
// from the gateway while the channel is open.
import { Button, Icon } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { useChannelParty } from './useChannelParty';
import './watchparty.css';

export function WatchPartyStrip({ channelId }: { channelId: string | undefined }) {
  const party = useChannelParty(channelId);
  const router = useRouter();
  if (!party) return null;
  const m = party.media;
  const n = party.participants.length;
  return (
    <aside className="wp-strip" aria-label="Watch party">
      <Icon name="film" size={20} />
      <span className="wp-strip-text">
        <strong>{m.title}</strong>
        {m.episode_label ? <span className="wp-muted"> · {m.episode_label}</span> : null}
        <span className="wp-muted">
          {' '}
          · {party.playback.playing ? 'playing' : 'paused'} · {n} watching
        </span>
      </span>
      <Button
        size="sm"
        onClick={() => router.history.push(`/cinema/party/${party.id}`)}
        aria-label={`Join the watch party for ${m.title}`}
      >
        Join
      </Button>
    </aside>
  );
}
