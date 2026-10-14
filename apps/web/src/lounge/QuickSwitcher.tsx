import { Icon, Modal, type IconName } from '@sideroom/ui';
import { useEffect, useId, useMemo, useState } from 'react';
import type { KeyboardEvent } from 'react';
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

export function QuickSwitcher({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const guilds = useGuilds();
  const channelsByGuild = useAllChannels();
  const dms = useDms();
  const users = useUsers();
  const me = useCurrentUser();
  const goto = useLoungeGoto();
  const recent = useLoungeUi((s) => s.recentSwitcher);
  const pushRecent = useLoungeUi((s) => s.pushRecent);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const listId = useId();

  useEffect(() => {
    if (open) {
      setQuery('');
      setActive(0);
    }
  }, [open]);

  const items = useMemo(
    () => buildSwitcherItems({ guilds, channelsByGuild, dms, users, meId: me.id }),
    [guilds, channelsByGuild, dms, users, me.id],
  );
  const results = useMemo(() => searchSwitcher(items, query, recent), [items, query, recent]);
  const clamped = Math.min(active, Math.max(0, results.length - 1));
  const hasPrefix = parseQuery(query).kinds !== null;

  const choose = (item: SwitcherItem | undefined) => {
    if (!item) return;
    pushRecent(item.key);
    goto(item.guildId, item.channelId);
    onClose();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActive((clamped + 1) % Math.max(1, results.length));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive((clamped - 1 + results.length) % Math.max(1, results.length));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      choose(results[clamped]);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('lounge.quickSwitcher.title')}
      size="md"
      className="lng-switcher"
    >
      <input
        data-autofocus
        className="lng-switcher-input"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-activedescendant={results[clamped] ? `${listId}-${clamped}` : undefined}
        aria-label={t('lounge.quickSwitcher.searchLabel')}
        placeholder={t('lounge.quickSwitcher.title')}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
      <ul
        id={listId}
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
            id={`${listId}-${i}`}
            role="option"
            aria-selected={i === clamped}
            className="lng-switcher-item"
            data-active={i === clamped || undefined}
            onMouseMove={() => setActive(i)}
            onClick={() => choose(r)}
          >
            <Icon name={ICON[r.kind]} size={20} />
            <span className="lng-switcher-label">{r.label}</span>
            <span className="lng-muted">{r.hint}</span>
          </li>
        ))}
      </ul>
      <p className="lng-switcher-tip">
        {hasPrefix || query
          ? t('lounge.quickSwitcher.tipActive')
          : t('lounge.quickSwitcher.tipIdle')}
      </p>
    </Modal>
  );
}
