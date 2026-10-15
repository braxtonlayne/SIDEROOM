import { Link } from '@tanstack/react-router';
import { Avatar, AvatarStack, Button, Icon, type IconName } from '@sideroom/ui';
import { useMemo } from 'react';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import { usePresenceMap, type Activity } from '../data';
import { HOME } from '../channels';
import { buildActiveCards, type ActiveCard, type VoiceRef, type Where } from './activeNowModel';
import type { RelationshipRow } from './api';
import { wireName } from './social';

const KIND_ICON: Record<Activity['kind'], IconName> = {
  playing: 'play',
  watching: 'tv',
  reading: 'book',
  listening: 'volume',
  custom: 'info',
};

/** Minutes since `startedAt`, as "for 12 minutes" / "for 2 hours"; empty when unknown. */
function useElapsed() {
  const t = useT();
  return (startedAt: number | undefined) => {
    if (!startedAt) return '';
    const mins = Math.max(0, Math.floor((Date.now() - startedAt) / 60_000));
    if (mins < 1) return t('lounge.activeNow.justStarted');
    if (mins < 60) return t('lounge.activeNow.forMinutes', { count: mins });
    return t('lounge.activeNow.forHours', { count: Math.floor(mins / 60) });
  };
}

function Header({ card }: { card: ActiveCard }) {
  const name = wireName(card.row.user);
  return (
    <div className="lng-ac-head">
      <Avatar name={name} size={32} status={card.status} />
      <span className="lng-friend-name">{name}</span>
    </div>
  );
}

function ChannelLink({ where, children }: { where: Where; children: React.ReactNode }) {
  return (
    <Link
      to="/channels/$guildId/$channelId"
      params={{ guildId: where.guildId ?? HOME, channelId: where.channelId }}
      className="lng-ac-link"
    >
      {children}
    </Link>
  );
}

function Card({ card }: { card: ActiveCard }) {
  const t = useT();
  const users = useStore((s) => s.users);
  const elapsed = useElapsed();
  const name = wireName(card.row.user);

  if (card.kind === 'voice') {
    const people = card.others.slice(0, 4).map((id) => ({
      id,
      name: users[id]?.global_name || users[id]?.username || '?',
    }));
    const where = card.where.guildName ?? t('lounge.activeNow.privateCall');
    return (
      <li className="lng-active-card lng-ac" data-kind="voice">
        <Header card={card} />
        <ChannelLink where={card.where}>
          <span className="lng-ac-line">
            <Icon name="volume" size={16} />
            {t('lounge.activeNow.inVoice', { channel: card.where.channelName ?? where })}
          </span>
          <span className="lng-friend-sub">{where}</span>
        </ChannelLink>
        {card.others.length > 0 && (
          <div className="lng-ac-people">
            <AvatarStack
              items={people}
              max={3}
              size={24}
              label={t('lounge.activeNow.alsoThere', { name })}
            />
            {card.others.length > people.length && (
              <span className="lng-friend-sub">+{card.others.length - people.length}</span>
            )}
          </div>
        )}
      </li>
    );
  }

  if (card.kind === 'stream')
    return (
      <li className="lng-active-card lng-ac" data-kind="stream">
        <Header card={card} />
        <ChannelLink where={card.where}>
          <span className="lng-ac-thumb" aria-hidden="true">
            <span className="lng-ac-live">{t('lounge.activeNow.live')}</span>
            <Icon name="screen-share" size={24} />
          </span>
          <span className="lng-ac-line">{t('lounge.activeNow.sharing')}</span>
          <span className="lng-friend-sub">
            {[card.where.channelName, card.where.guildName].filter(Boolean).join(' · ')}
          </span>
        </ChannelLink>
      </li>
    );

  if (card.kind === 'party')
    return (
      <li className="lng-active-card lng-ac" data-kind="party">
        <Header card={card} />
        <span className="lng-ac-line">
          <Icon name="tv" size={16} />
          {t('lounge.activeNow.party', { title: card.activity.text })}
        </span>
        <Link to="/cinema/party/$id" params={{ id: card.partyId }} className="lng-ac-join">
          <Button size="sm" variant="secondary" tabIndex={-1}>
            {t('lounge.activeNow.join')}
          </Button>
        </Link>
      </li>
    );

  const a = card.activity;
  const line = {
    playing: t('lounge.friends.activityPlaying', { text: a.text }),
    watching: t('lounge.friends.activityWatching', { text: a.text }),
    reading: t('lounge.friends.activityReading', { text: a.text }),
    listening: t('lounge.friends.activityListening', { text: a.text }),
    custom: a.text,
  }[a.kind];
  return (
    <li className="lng-active-card lng-ac" data-kind={a.kind}>
      <Header card={card} />
      <span className="lng-ac-line">
        <Icon name={KIND_ICON[a.kind]} size={16} />
        {line}
      </span>
      {a.details && <span className="lng-friend-sub">{a.details}</span>}
      {a.platform && a.kind === 'playing' && (
        <span className="lng-friend-sub" data-platform={a.platform}>
          {t('lounge.activeNow.onPlatform', { platform: a.platform })}
        </span>
      )}
      {elapsed(a.startedAt) && <span className="lng-friend-sub">{elapsed(a.startedAt)}</span>}
    </li>
  );
}

/** The Active Now column of the Friends page (LNG-FRD-013, empty state LNG-FRD-014). */
export function ActiveNow({ friends }: { friends: RelationshipRow[] }) {
  const t = useT();
  const presence = usePresenceMap();
  const voiceStates = useStore((s) => s.voiceStates);
  const guilds = useStore((s) => s.guilds);
  const channels = useStore((s) => s.channels);
  const cards = useMemo(() => {
    const voice: VoiceRef[] = [];
    for (const [guildId, byUser] of Object.entries(voiceStates))
      for (const v of Object.values(byUser))
        if (v.channel_id)
          voice.push({
            userId: v.user_id,
            guildId: guildId === '@me' ? null : guildId,
            channelId: v.channel_id,
            stream: !!v.self_stream,
          });
    return buildActiveCards({
      friends,
      presence,
      voice,
      guildName: (id) => guilds[id]?.name ?? null,
      channelName: (id) => channels[id]?.name ?? null,
    });
  }, [friends, presence, voiceStates, guilds, channels]);
  return (
    <aside className="lng-active-now" aria-label={t('lounge.friends.activeNow')}>
      <h2 className="lng-friends-h">{t('lounge.friends.activeNow')}</h2>
      {cards.length === 0 ? (
        <div className="lng-active-empty">
          <p className="lng-active-title">{t('lounge.friends.quietTitle')}</p>
          <p className="lng-muted">{t('lounge.friends.quietBody')}</p>
        </div>
      ) : (
        <ul className="lng-active-list">
          {cards.map((c) => (
            <Card key={c.row.id} card={c} />
          ))}
        </ul>
      )}
    </aside>
  );
}
