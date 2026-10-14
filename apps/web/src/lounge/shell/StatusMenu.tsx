import { Avatar, DropdownMenu, type MenuEntry } from '@sideroom/ui';
import { useEffect } from 'react';
import { t, useT, type MessageKey } from '../../i18n';
import { useLoungeUi } from '../store';
import { useSelfPresence, type SelfStatus } from './selfPresence';

const LABEL: Record<SelfStatus, MessageKey> = {
  online: 'lounge.statusMenu.online',
  idle: 'lounge.statusMenu.idle',
  dnd: 'lounge.statusMenu.dnd',
  invisible: 'lounge.statusMenu.invisible',
};

const DURATIONS: { labelKey: MessageKey; seconds: number | null }[] = [
  { labelKey: 'lounge.statusMenu.for15m', seconds: 900 },
  { labelKey: 'lounge.statusMenu.for1h', seconds: 3600 },
  { labelKey: 'lounge.statusMenu.for3h', seconds: 10800 },
  { labelKey: 'lounge.statusMenu.for8h', seconds: 28800 },
  { labelKey: 'lounge.statusMenu.for24h', seconds: 86400 },
  { labelKey: 'lounge.statusMenu.forever', seconds: null },
];

export const statusLabel = (s: SelfStatus) => t(LABEL[s]);

/**
 * The avatar in the user panel: opens the status picker (LNG-HOME-011). Do Not Disturb and Invisible offer
 * durations; the server reverts to online afterwards.
 */
export function StatusMenu({ name }: { name: string }) {
  const t = useT();
  const status = useSelfPresence((s) => s.status);
  const customText = useSelfPresence((s) => s.customText);
  const load = useSelfPresence((s) => s.load);
  const update = useSelfPresence((s) => s.update);
  const openDialog = useLoungeUi((s) => s.openDialog);

  useEffect(() => {
    void load();
  }, [load]);

  const set = (next: SelfStatus, seconds: number | null = null) =>
    void update({ status: next, duration_seconds: seconds }).catch(() => undefined);
  const timed = (next: SelfStatus, label: string): MenuEntry => ({
    type: 'submenu',
    label,
    items: DURATIONS.map((d) => ({ label: t(d.labelKey), onSelect: () => set(next, d.seconds) })),
  });

  const items: MenuEntry[] = [
    {
      label: customText ? t('lounge.statusMenu.editCustom') : t('lounge.statusMenu.setCustom'),
      onSelect: () => openDialog({ kind: 'customStatus' }),
    },
    { type: 'separator' },
    { label: t('lounge.statusMenu.online'), onSelect: () => set('online') },
    { label: t('lounge.statusMenu.idle'), onSelect: () => set('idle') },
    timed('dnd', t('lounge.statusMenu.dnd')),
    timed('invisible', t('lounge.statusMenu.invisible')),
  ];

  return (
    <DropdownMenu
      label={t('lounge.statusMenu.setStatus')}
      items={items}
      trigger={
        <button
          type="button"
          className="lng-user-avatar"
          aria-label={t('lounge.statusMenu.currently', { status: t(LABEL[status]) })}
        >
          <Avatar name={name} size={32} status={status === 'invisible' ? 'offline' : status} />
        </button>
      }
    />
  );
}
