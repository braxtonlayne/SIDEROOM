// G-79 (D-192, G-25): the two account switches the server reads from the settings blob.
import { Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { useEnsureUserSettings } from '../../../lounge/msg/remoteImages';
import {
  PLATFORM_DM,
  STAFF_SHOW_BADGE,
  setAccountSwitch,
  useAccountSwitch,
  useAccountSwitchOff,
  type AccountSwitch,
} from '../accountSwitches';
import { errorText } from '../api';
import { Card } from '../parts';

function SwitchRow({
  sw,
  label,
  description,
}: {
  sw: AccountSwitch;
  label: string;
  description: string;
}) {
  const t = useT();
  useEnsureUserSettings();
  const on = useAccountSwitch(sw);
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <Switch
        label={label}
        description={description}
        checked={on}
        onChange={(e) => {
          setError(null);
          setAccountSwitch(sw, e.target.checked).catch((err: unknown) => setError(errorText(err)));
        }}
      />
      {error && (
        <Notice variant="danger" title={t('settings.accountSwitch.saveError')}>
          {error}
        </Notice>
      )}
    </>
  );
}

/** Settings → Notifications: platform announcements by DM (`notifications.platform_dm`). */
export function PlatformDmCard() {
  const t = useT();
  return (
    <Card id="us-platform-dm" title={t('settings.platformDm.title')}>
      <SwitchRow
        sw={PLATFORM_DM}
        label={t('settings.platformDm.label')}
        description={t('settings.platformDm.desc')}
      />
    </Card>
  );
}

/**
 * Settings → Profiles: show the SIDE ROOM Staff badge (`staff.show_badge`). Only staff see it: the profile
 * carries the `staff` or `owner` badge while it is shown; once hidden the server drops the badge, so a stored
 * `false` keeps the switch visible to turn it back on.
 */
export function StaffBadgeCard({ badges }: { badges: readonly { id: string }[] | undefined }) {
  const t = useT();
  useEnsureUserSettings();
  const off = useAccountSwitchOff(STAFF_SHOW_BADGE);
  const staff = (badges ?? []).some((b) => b.id === 'staff' || b.id === 'owner');
  if (!staff && !off) return null;
  return (
    <Card id="us-staff-badge" title={t('settings.staffBadge.title')}>
      <SwitchRow
        sw={STAFF_SHOW_BADGE}
        label={t('settings.staffBadge.label')}
        description={t('settings.staffBadge.desc')}
      />
    </Card>
  );
}
