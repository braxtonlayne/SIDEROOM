import { Icon, type IconName } from '@sideroom/ui';
import { useMemo } from 'react';
import { useT } from '../i18n';
import { useAllChannels, useCurrentUser, useDms, useGuilds, useUsers } from './data';
import { useLoungeGoto } from './nav';
import {
  buildSwitcherItems,
  parseQuery,
  searchSwitcher,
  type SwitcherItem,
  type SwitcherKind,
} from './switcherSearch';
import { useLoungeUi } from './store';

const ICON: Record<SwitcherKind, IconName> = {
  dm: 'user',
  user: 'user',
  text: 'hash',
  voice: 'volume',
  guild: 'home',
};

/**
 * UTL-SHL-001: the quick switcher is the "Go to" tab of the utility drawer (`tools/drawer`). The drawer owns the
 * search box, the dialog and the keys; this file keeps what is specific to going somewhere: the ranked results
 * for a query, choosing one, and the list. The behaviour is the one the modal had: the same ranking
 * (`switcherSearch.ts`), the same `#`, `@`, `!` and `*` prefixes, recents, the same navigation.
 */
export function useGotoResults(query: string, onDone: () => void) {
  const guilds = useGuilds();
  const channelsByGuild = useAllChannels();
  const dms = useDms();
  const users = useUsers();
  const me = useCurrentUser();
  const goto = useLoungeGoto();
  const recent = useLoungeUi((s) => s.recentSwitcher);
  const pushRecent = useLoungeUi((s) => s.pushRecent);

  const items = useMemo(
    () => buildSwitcherItems({ guilds, channelsByGuild, dms, users, meId: me.id }),
    [guilds, channelsByGuild, dms, users, me.id],
  );
  const results = useMemo(() => searchSwitcher(items, query, recent), [items, query, recent]);
  const hasPrefix = parseQuery(query).kinds !== null;

  const choose = (item: SwitcherItem | undefined) => {
    if (!item) return;
    pushRecent(item.key);
    goto(item.guildId, item.channelId);
    onDone();
  };
  return { results, hasPrefix, choose };
}

export function GotoList({
  id,
  results,
  active,
  onActive,
  onChoose,
}: {
  id: string;
  results: SwitcherItem[];
  active: number;
  onActive: (i: number) => void;
  onChoose: (item: SwitcherItem) => void;
}) {
  const t = useT();
  return (
    <ul
      id={id}
      role="listbox"
      aria-label={t('lounge.quickSwitcher.results')}
      className="lng-switcher-list"
    >
      {results.length === 0 && (
        <li className="lng-muted lng-pad" role="presentation">
          {t('lounge.quickSwitcher.noResults')}
        </li>
      )}
      {results.map((r, i) => (
        <li
          key={r.key}
          id={`${id}-${i}`}
          role="option"
          aria-selected={i === active}
          className="lng-switcher-item"
          data-active={i === active || undefined}
          onMouseMove={() => onActive(i)}
          onClick={() => onChoose(r)}
        >
          <Icon name={ICON[r.kind]} size={20} />
          <span className="lng-switcher-label">{r.label}</span>
          <span className="lng-muted">{r.hint}</span>
        </li>
      ))}
    </ul>
  );
}
