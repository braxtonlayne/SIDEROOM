import { CounterBadge, Icon, RailPill } from '@sideroom/ui';
import { useT } from '../i18n';
import { FIXTURE_SERVERS, type FixtureServer } from './fixtures';
import { previewHint } from './state';

function acronym(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .map((w) => w[0])
    .join('')
    .slice(0, 3)
    .toUpperCase();
}

function SampleServer({ server }: { server: FixtureServer }) {
  const t = useT();
  const label =
    server.name +
    (server.mentions
      ? t('lounge.guildRail.mentionsSuffix', { count: server.mentions })
      : server.unread
        ? t('lounge.guildRail.unreadSuffix')
        : '') +
    (server.voice ? t('lounge.guildRail.inVoiceSuffix') : '') +
    t('showroom.rail.sampleSuffix');
  return (
    <li>
      <div className="lng-rail-item" data-muted={server.muted || undefined} data-showroom="server">
        <RailPill state={server.unread && !server.muted ? 'unread' : 'none'} />
        {/* A button, never a link: a sample server has nowhere to go, and nothing is sent. */}
        <button
          type="button"
          className="lng-guild"
          title={label}
          aria-label={label}
          onClick={() => previewHint(t('showroom.hint.server', { name: server.name }))}
        >
          <span className="lng-guild-acronym" aria-hidden="true" data-text={acronym(server.name)} />
          {server.voice > 0 && (
            <span className="lng-guild-voice" title={t('showroom.rail.voice', { n: server.voice })}>
              <Icon name="volume" size={16} />
            </span>
          )}
        </button>
        {!server.muted && (
          <CounterBadge
            count={server.mentions}
            noun={t('lounge.guildRail.mentionsNoun')}
            className="lng-mention-badge"
          />
        )}
      </div>
    </li>
  );
}

/** The ten sample servers, under the real ones, behind a separator. */
export default function PreviewRailItems() {
  return (
    <>
      <li aria-hidden="true">
        <hr className="lng-rail-sep" />
      </li>
      {FIXTURE_SERVERS.map((s) => (
        <SampleServer key={s.id} server={s} />
      ))}
    </>
  );
}
