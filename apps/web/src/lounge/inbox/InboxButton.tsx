import { Icon, Popover } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import type { Message } from '../../store';
import { HOME } from '../channels';
import { useLoungeGoto, useLoungeGotoMessage, useLoungeRoute } from '../nav';
import { unreadRows } from './unreads';

type Tab = 'mentions' | 'unreads';

function Mentions({ onGo }: { onGo: () => void }) {
  const t = useT();
  const route = useLoungeRoute();
  const goto = useLoungeGotoMessage();
  const channels = useStore((s) => s.channels);
  const [roles, setRoles] = useState(true);
  const [everyone, setEveryone] = useState(true);
  const [thisServer, setThisServer] = useState(false);
  const [list, setList] = useState<Message[] | null>(null);
  const [failed, setFailed] = useState(false);
  const guildId = route.guildId && route.guildId !== HOME ? route.guildId : undefined;
  useEffect(() => {
    let live = true;
    const q = new URLSearchParams({ roles: String(roles), everyone: String(everyone) });
    if (thisServer && guildId) q.set('guild_id', guildId);
    apiFetch<Message[]>(`/users/@me/mentions?${q}`, { retries: 0 })
      .then((m) => {
        if (!live) return;
        setFailed(false);
        setList(m);
      })
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [roles, everyone, thisServer, guildId]);
  return (
    <div className="lng-inbox-body">
      <div className="lng-inbox-filters">
        {guildId && (
          <label>
            <input
              type="checkbox"
              checked={thisServer}
              onChange={(e) => setThisServer(e.target.checked)}
            />{' '}
            {t('lounge.inbox.thisServerOnly')}
          </label>
        )}
        <label>
          <input type="checkbox" checked={roles} onChange={(e) => setRoles(e.target.checked)} />{' '}
          {t('lounge.inbox.includeRoles')}
        </label>
        <label>
          <input
            type="checkbox"
            checked={everyone}
            onChange={(e) => setEveryone(e.target.checked)}
          />{' '}
          {t('lounge.inbox.includeEveryone')}
        </label>
      </div>
      {failed && <p className="lng-muted">{t('lounge.inbox.loadFailed')}</p>}
      {!failed && list === null && <p className="lng-muted">{t('lounge.inbox.loading')}</p>}
      {list?.length === 0 && <p className="lng-muted">{t('lounge.inbox.noMentions')}</p>}
      <ul className="lng-inbox-list">
        {list?.map((m) => {
          const ch = channels[m.channel_id];
          const where = ch?.name ? `#${ch.name}` : t('lounge.inbox.directMessage');
          return (
            <li key={m.id}>
              <button
                type="button"
                className="lng-inbox-item"
                onClick={() => {
                  goto(m.guild_id ?? HOME, m.channel_id, m.id);
                  onGo();
                }}
              >
                <strong>{m.author.global_name ?? m.author.username}</strong>
                <span className="lng-muted"> {where}</span>
                <span className="lng-inbox-snippet">{m.content.slice(0, 140)}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function Unreads({ onGo }: { onGo: () => void }) {
  const t = useT();
  const goto = useLoungeGoto();
  const channels = useStore((s) => s.channels);
  const readStates = useStore((s) => s.readStates);
  const ack = useStore((s) => s.ackChannel);
  const rows = unreadRows(channels, readStates);
  if (rows.length === 0) return <p className="lng-muted">{t('lounge.inbox.noUnreads')}</p>;
  return (
    <div className="lng-inbox-body">
      <ul className="lng-inbox-list">
        {rows.map((r) => (
          <li key={r.channelId} className="lng-inbox-row">
            <button
              type="button"
              className="lng-inbox-item"
              onClick={() => {
                goto(r.guildId ?? HOME, r.channelId);
                onGo();
              }}
            >
              <strong>{r.guildId ? `#${r.name}` : r.name}</strong>
              {r.mentions > 0 && (
                <span className="lng-inbox-count">
                  {t('lounge.inbox.mentionCount', { count: r.mentions })}
                </span>
              )}
            </button>
            <button
              type="button"
              className="lng-inbox-mark"
              aria-label={t('lounge.inbox.markRead', { name: r.name })}
              onClick={() => ack(r.channelId)}
            >
              {t('lounge.inbox.markReadShort')}
            </button>
          </li>
        ))}
      </ul>
      <button
        type="button"
        className="lng-inbox-markall"
        onClick={() => rows.forEach((r) => ack(r.channelId))}
      >
        {t('lounge.inbox.markAllRead')}
      </button>
    </div>
  );
}

/** Inbox popout (LNG-NTF-006): Mentions and Unreads. "For You" needs aggregation not built yet. */
export function InboxButton({ iconSize = 20 }: { iconSize?: 16 | 20 }) {
  const t = useT();
  const [tab, setTab] = useState<Tab>('mentions');
  const [closeKey, setCloseKey] = useState(0);
  const close = () => setCloseKey((n) => n + 1);
  return (
    <Popover
      key={closeKey}
      label={t('lounge.inbox.title')}
      placement="bottom-end"
      trigger={
        <button type="button" className="lng-icon-btn" aria-label={t('lounge.inbox.title')}>
          <Icon name="bell" size={iconSize} />
        </button>
      }
    >
      <div className="lng-inbox">
        <div role="tablist" aria-label={t('lounge.inbox.title')} className="lng-expr-tabs">
          {(['mentions', 'unreads'] as const).map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
            >
              {id === 'mentions' ? t('lounge.inbox.tabMentions') : t('lounge.inbox.tabUnreads')}
            </button>
          ))}
        </div>
        <div role="tabpanel">
          {tab === 'mentions' ? <Mentions onGo={close} /> : <Unreads onGo={close} />}
        </div>
      </div>
    </Popover>
  );
}
