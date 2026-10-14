import { Avatar, Icon, Popover } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useUsers } from '../data';

interface WireMember {
  user_id: string;
}

/** Members of a thread, in a popover on the thread header (LNG-THR-005). */
export function ThreadMembers({ threadId, count }: { threadId: string; count: number }) {
  const t = useT();
  const users = useUsers();
  const [open, setOpen] = useState(false);
  const [ids, setIds] = useState<string[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setError(false);
    apiFetch<WireMember[]>(`/channels/${threadId}/thread-members?limit=100`, { retries: 0 })
      .then((list) => live && setIds(list.map((m) => m.user_id)))
      .catch(() => live && setError(true));
    return () => {
      live = false;
    };
  }, [open, threadId]);

  return (
    <Popover
      label={t('lounge.threadMembers.title')}
      placement="bottom-end"
      onOpenChange={setOpen}
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.threadMembers.button', { count })}
          title={t('lounge.threadMembers.button', { count })}
        >
          <Icon name="users" size={20} />
        </button>
      }
    >
      <div className="lng-threads-pop">
        <h2 className="lng-pop-title">{t('lounge.threadMembers.title')}</h2>
        {error && (
          <p className="lng-muted" role="alert">
            {t('lounge.threadMembers.error')}
          </p>
        )}
        {ids === null && !error && (
          <p className="lng-muted" role="status">
            {t('lounge.threadMembers.loading')}
          </p>
        )}
        <ul className="lng-thread-members">
          {ids?.map((id) => (
            <li key={id}>
              <Avatar name={users[id]?.displayName ?? id} size={24} />
              <span>{users[id]?.displayName ?? t('lounge.threadMembers.unknown')}</span>
            </li>
          ))}
        </ul>
      </div>
    </Popover>
  );
}
