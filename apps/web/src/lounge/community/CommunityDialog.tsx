// Community dialog: the member-facing side of the feature modules a server has switched on (leveling, suggestions,
// tickets, birthdays). It opens from the server menu and from /rank, /suggest and /ticket; only tabs whose module
// is on are listed.
import { Modal, Tabs } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useUsers } from '../data';
import { getUpcomingBirthdays, type UpcomingBirthday } from '../features/api';
import { MODULE, useGuildModules } from '../features/useModules';
import { Leveling } from './Leveling';
import { Suggestions } from './Suggestions';
import { Tickets } from './Tickets';
import { useLoad } from './useLoad';
import './community.css';
import { formatDate } from '../../i18n';

export type CommunityTab = 'rank' | 'suggestions' | 'tickets' | 'birthdays';

export function CommunityDialog({
  guildId,
  tab,
  onClose,
}: {
  guildId: string;
  tab?: CommunityTab | undefined;
  onClose: () => void;
}) {
  const t = useT();
  const modules = useGuildModules(guildId);
  const tabs = [
    {
      id: 'rank' as const,
      on: !!modules?.[MODULE.leveling],
      label: t('lounge.community.tabRank'),
      content: <Leveling guildId={guildId} />,
    },
    {
      id: 'suggestions' as const,
      on: !!modules?.[MODULE.suggestions],
      label: t('lounge.community.tabSuggestions'),
      content: <Suggestions guildId={guildId} />,
    },
    {
      id: 'tickets' as const,
      on: !!modules?.[MODULE.tickets],
      label: t('lounge.community.tabTickets'),
      content: <Tickets guildId={guildId} onNavigate={onClose} />,
    },
    {
      id: 'birthdays' as const,
      on: !!modules?.[MODULE.birthdays],
      label: t('lounge.community.tabBirthdays'),
      content: <Birthdays guildId={guildId} />,
    },
  ].filter((x) => x.on);

  return (
    <Modal open onClose={onClose} size="lg" title={t('lounge.community.title')}>
      {modules === undefined ? (
        <p className="lng-muted" role="status">
          {t('lounge.community.loading')}
        </p>
      ) : tabs.length === 0 ? (
        <p className="lng-muted">{t('lounge.community.nothingOn')}</p>
      ) : (
        <Tabs
          label={t('lounge.community.tabs')}
          variant="pill"
          defaultValue={tabs.some((x) => x.id === tab) ? tab : tabs[0]!.id}
          items={tabs.map(({ id, label, content }) => ({ id, label, content }))}
        />
      )}
    </Modal>
  );
}

function Birthdays({ guildId }: { guildId: string }) {
  const t = useT();
  const users = useUsers();
  const list = useLoad(() => getUpcomingBirthdays(guildId), `bday-${guildId}`);
  if (list.error)
    return (
      <p className="lng-failed" role="alert">
        {t('lounge.community.birthdays.error')}
      </p>
    );
  if (list.loading)
    return (
      <p className="lng-muted" role="status">
        {t('lounge.community.loading')}
      </p>
    );
  const rows: UpcomingBirthday[] = list.data ?? [];
  if (rows.length === 0)
    return <p className="lng-muted">{t('lounge.community.birthdays.empty')}</p>;
  return (
    <ul className="lng-saved-list" aria-label={t('lounge.community.birthdays.listLabel')}>
      {rows.map((b) => (
        <li key={b.user_id} className="lng-saved-item">
          <strong>
            {users[b.user_id]?.displayName ?? t('lounge.community.suggestions.someone')}
          </strong>{' '}
          <span className="lng-muted">
            {formatDate(new Date(Date.UTC(2000, b.month - 1, b.day)), {
              month: 'long',
              day: 'numeric',
              timeZone: 'UTC',
            })}
          </span>
        </li>
      ))}
    </ul>
  );
}
