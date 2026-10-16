// "Activities" in the voice call bar (DEV-ACT-004). Hidden unless the server runs Activities
// (`[activities] enabled`). Opens the shelf; with an Activity already running in the channel it joins that one.
import { Icon } from '@sideroom/ui';
import { lazy, Suspense, useState } from 'react';
import { useT } from '../i18n';
import { httpActivityApi } from './api';
import { useChannelActivity } from './hooks';

const Launcher = lazy(() =>
  import('./ActivityLauncher').then((m) => ({ default: m.ActivityLauncher })),
);

export function ActivityButton({ channelId }: { channelId: string }) {
  const t = useT();
  const { enabled, instance } = useChannelActivity(channelId);
  const [open, setOpen] = useState(false);
  if (!enabled) return null;
  const label = instance
    ? t('activities.button.join', { name: instance.application_name })
    : t('activities.button.start');
  return (
    <>
      <button
        type="button"
        className="vc-ctl"
        aria-label={label}
        title={label}
        data-active={instance || undefined}
        onClick={() => {
          if (instance) void httpActivityApi.join(instance.id).catch(() => undefined);
          else setOpen(true);
        }}
      >
        <Icon name="sparkle" size={24} />
      </button>
      {open && (
        <Suspense fallback={null}>
          <Launcher open onClose={() => setOpen(false)} channelId={channelId} />
        </Suspense>
      )}
    </>
  );
}
