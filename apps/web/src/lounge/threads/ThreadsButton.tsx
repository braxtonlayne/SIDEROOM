import { Icon, Popover } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useLoungeGotoThread } from '../nav';
import { threadsOf, useThreads, type ThreadInfo } from './threadStore';

/** Threads button in the channel header: active and archived threads of this channel (LNG-THR-001 UI). */
export function ThreadsButton({ guildId, channelId }: { guildId: string; channelId: string }) {
  const byId = useThreads((s) => s.byId);
  const loaded = useThreads((s) => !!s.loadedGuilds[guildId]);
  const [tab, setTab] = useState<'active' | 'archived'>('active');
  const [open, setOpen] = useState(false);
  const [error, setError] = useState(false);
  const [more, setMore] = useState(false);
  const openThread = useLoungeGotoThread();

  const active = useMemo(() => threadsOf(byId, channelId), [byId, channelId]);
  const archived = useMemo(() => threadsOf(byId, channelId, { archived: true }), [byId, channelId]);

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

  const list: ThreadInfo[] = tab === 'active' ? active : archived;
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
      label="Threads"
      placement="bottom-end"
      onOpenChange={setOpen}
      trigger={
        <button type="button" className="lng-icon-btn" aria-label="Threads" title="Threads">
          <Icon name="list" size={20} />
        </button>
      }
    >
      <div className="lng-threads-pop">
        <div role="tablist" aria-label="Thread lists" className="lng-threads-tabs">
          {(['active', 'archived'] as const).map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              onClick={() => setTab(t)}
            >
              {t === 'active' ? 'Active' : 'Archived'}
            </button>
          ))}
        </div>
        {error && (
          <p className="lng-muted" role="alert">
            Could not load threads.
          </p>
        )}
        {list.length === 0 && !error && (
          <p className="lng-muted">
            {tab === 'active'
              ? 'No active threads. Start one from a message.'
              : 'No archived threads.'}
          </p>
        )}
        <ul className="lng-threads-list">
          {list.map((t) => (
            <li key={t.id}>
              <button type="button" onClick={() => openThread(guildId, channelId, t.id)}>
                <strong>{t.name}</strong>
                <span className="lng-muted">
                  {t.messageCount} {t.messageCount === 1 ? 'message' : 'messages'}
                  {t.joined ? ' · Following' : ''}
                </span>
              </button>
            </li>
          ))}
        </ul>
        {tab === 'archived' && more && (
          <button type="button" className="lng-link-btn" onClick={loadMore}>
            Load more
          </button>
        )}
      </div>
    </Popover>
  );
}
