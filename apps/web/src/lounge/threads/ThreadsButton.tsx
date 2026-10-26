import { Icon, Popover } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { useViewAsThreadParent } from '../../settings/server/viewAsMode';
import { useLoungeGotoThread } from '../nav';
import { threadsOf, useThreads, type ThreadInfo } from './threadStore';

/** Threads button in the channel header: active and archived threads of this channel (LNG-THR-001 UI). */
export function ThreadsButton({ guildId, channelId }: { guildId: string; channelId: string }) {
  const t = useT();
  const byId = useThreads((s) => s.byId);
  const loaded = useThreads((s) => !!s.loadedGuilds[guildId]);
  const [tab, setTab] = useState<'joined' | 'active' | 'archived'>('active');
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [error, setError] = useState(false);
  const [more, setMore] = useState(false);
  const openThread = useLoungeGotoThread();

  const parentShown = useViewAsThreadParent(guildId);
  const active = useMemo(
    () => (parentShown(channelId) ? threadsOf(byId, channelId) : []),
    [byId, channelId, parentShown],
  );
  const archived = useMemo(
    () => (parentShown(channelId) ? threadsOf(byId, channelId, { archived: true }) : []),
    [byId, channelId, parentShown],
  );

  useEffect(() => {
    if (!open) return;
    setError(false);
    if (!loaded)
      useThreads
        .getState()
        .loadActive(guildId)
        .catch(() => setError(true));
  }, [open, loaded, guildId]);
  useEffect(() => {
    if (!open || tab !== 'archived') return;
    useThreads
      .getState()
      .loadArchived(channelId)
      .then(setMore)
      .catch(() => setError(true));
  }, [open, tab, channelId]);

  const base: ThreadInfo[] =
    tab === 'archived' ? archived : tab === 'joined' ? active.filter((th) => th.joined) : active;
  const needle = query.trim().toLowerCase();
  const list = needle ? base.filter((th) => th.name.toLowerCase().includes(needle)) : base;
  const loadMore = () => {
    const last = archived[archived.length - 1]?.archiveTimestamp;
    useThreads
      .getState()
      .loadArchived(channelId, last ?? undefined)
      .then(setMore)
      .catch(() => setError(true));
  };

  return (
    <Popover
      label={t('lounge.threadsButton.title')}
      placement="bottom-end"
      onOpenChange={setOpen}
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.threadsButton.title')}
          title={t('lounge.threadsButton.title')}
        >
          <Icon name="list" size={20} />
        </button>
      }
    >
      <div className="lng-threads-pop">
        <div
          role="tablist"
          aria-label={t('lounge.threadsButton.lists')}
          className="lng-threads-tabs"
        >
          {(['joined', 'active', 'archived'] as const).map((which) => (
            <button
              key={which}
              type="button"
              role="tab"
              aria-selected={tab === which}
              onClick={() => setTab(which)}
            >
              {which === 'active'
                ? t('lounge.threadsButton.tabActive')
                : which === 'joined'
                  ? t('lounge.threadsButton.tabJoined')
                  : t('lounge.threadsButton.tabArchived')}
            </button>
          ))}
        </div>
        <input
          type="search"
          className="lng-threads-search"
          value={query}
          placeholder={t('lounge.threadsButton.search')}
          aria-label={t('lounge.threadsButton.search')}
          onChange={(e) => setQuery(e.target.value)}
        />
        {error && (
          <p className="lng-muted" role="alert">
            {t('lounge.threadsButton.loadError')}
          </p>
        )}
        {list.length === 0 && !error && (
          <p className="lng-muted">
            {needle
              ? t('lounge.threadsButton.emptySearch')
              : tab === 'archived'
                ? t('lounge.threadsButton.emptyArchived')
                : tab === 'joined'
                  ? t('lounge.threadsButton.emptyJoined')
                  : t('lounge.threadsButton.emptyActive')}
          </p>
        )}
        <ul className="lng-threads-list">
          {list.map((th) => (
            <li key={th.id}>
              <button type="button" onClick={() => openThread(guildId, channelId, th.id)}>
                <strong>
                  {th.isPrivate && <Icon name="lock" size={16} />} {th.name}
                </strong>
                <span className="lng-muted">
                  {t('lounge.threadsButton.messageCount', { count: th.messageCount })}
                  {th.joined ? t('lounge.threadsButton.followingSuffix') : ''}
                </span>
              </button>
            </li>
          ))}
        </ul>
        {tab === 'archived' && more && (
          <button type="button" className="lng-link-btn" onClick={loadMore}>
            {t('lounge.threadsButton.loadMore')}
          </button>
        )}
      </div>
    </Popover>
  );
}
