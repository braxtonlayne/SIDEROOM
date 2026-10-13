import { Avatar, DropdownMenu, type MenuEntry } from '@sideroom/ui';
import { useEffect } from 'react';
import { useLoungeUi } from '../store';
import { useSelfPresence, type SelfStatus } from './selfPresence';

const LABEL: Record<SelfStatus, string> = {
  online: 'Online',
  idle: 'Idle',
  dnd: 'Do Not Disturb',
  invisible: 'Invisible',
};

const DURATIONS: { label: string; seconds: number | null }[] = [
  { label: 'For 15 Minutes', seconds: 900 },
  { label: 'For 1 Hour', seconds: 3600 },
  { label: 'For 3 Hours', seconds: 10800 },
  { label: 'For 8 Hours', seconds: 28800 },
  { label: 'For 24 Hours', seconds: 86400 },
  { label: 'Forever', seconds: null },
];

export const statusLabel = (s: SelfStatus) => LABEL[s];

/**
 * The avatar in the user panel: opens the status picker (LNG-HOME-011). Do Not Disturb and Invisible offer
 * durations; the server reverts to online afterwards.
 */
export function StatusMenu({ name }: { name: string }) {
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
    items: DURATIONS.map((d) => ({ label: d.label, onSelect: () => set(next, d.seconds) })),
  });

  const items: MenuEntry[] = [
    {
      label: customText ? 'Edit Custom Status' : 'Set Custom Status',
      onSelect: () => openDialog({ kind: 'customStatus' }),
    },
    { type: 'separator' },
    { label: 'Online', onSelect: () => set('online') },
    { label: 'Idle', onSelect: () => set('idle') },
    timed('dnd', 'Do Not Disturb'),
    timed('invisible', 'Invisible'),
  ];

  return (
    <DropdownMenu
      label="Set status"
      items={items}
      trigger={
        <button
          type="button"
          className="lng-user-avatar"
          aria-label={`Set status, currently ${LABEL[status]}`}
        >
          <Avatar name={name} size={32} status={status === 'invisible' ? 'offline' : status} />
        </button>
      }
    />
  );
}
