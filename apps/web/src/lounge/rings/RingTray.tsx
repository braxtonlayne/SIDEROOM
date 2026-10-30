// The strip of rings at the top of the home sidebar: an add button, then everyone with a live ring. It renders
// nothing when the feed could not be loaded (rings switched off, signed out), so a missing module leaves no hole.
import { Avatar, Icon } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { useCurrentUser, useUsers } from '../data';
import { RingAvatar } from './RingAvatar';
import { RingComposer } from './RingComposer';
import { useRingFeed, useRingFeedStore } from './store';
import './rings.css';

export function RingTray() {
  const t = useT();
  const feed = useRingFeed();
  const loaded = useRingFeedStore((s) => s.entries !== null);
  const users = useUsers();
  const me = useCurrentUser();
  const [composing, setComposing] = useState(false);
  if (!loaded) return null;
  const others = feed.filter((e) => e.author_id !== me.id);
  const mine = feed.some((e) => e.author_id === me.id);
  return (
    <section className="rng-tray" aria-label={t('lounge.rings.tray.label')}>
      <div className="rng-tray-item">
        {mine ? (
          <RingAvatar userId={me.id} name={me.displayName}>
            <Avatar name={me.displayName} src={me.avatarUrl ?? null} size={40} />
          </RingAvatar>
        ) : null}
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.rings.tray.add')}
          onClick={() => setComposing(true)}
        >
          <Icon name="plus" size={20} />
        </button>
      </div>
      {others.map((e) => {
        const u = users[e.author_id];
        const name = u?.displayName ?? t('lounge.rings.viewer.unknownPerson');
        return (
          <div className="rng-tray-item" key={e.author_id}>
            <RingAvatar userId={e.author_id} name={name}>
              <Avatar name={name} src={u?.avatarUrl ?? null} size={40} />
            </RingAvatar>
            <span className="rng-tray-name">{name}</span>
          </div>
        );
      })}
      {composing && <RingComposer onClose={() => setComposing(false)} />}
    </section>
  );
}
