// The Activity of a voice channel, shown above the call stage while you are in the call (DEV-ACT-004): the
// frame when you joined it, a "Join" strip when you have not.
import { Button, Icon } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { useCurrentUser } from '../lounge/data';
import { httpActivityApi, type ActivityApi } from './api';
import { ActivityFrame } from './ActivityFrame';
import { useChannelActivity } from './hooks';
import './activities.css';

export function ActivityPanel({
  channelId,
  channelName,
  here,
  api = httpActivityApi,
}: {
  channelId: string;
  channelName: string;
  /** You are in the call. */
  here: boolean;
  api?: ActivityApi;
}) {
  const t = useT();
  const me = useCurrentUser();
  const { enabled, instance } = useChannelActivity(channelId, api);
  const [busy, setBusy] = useState(false);
  if (!enabled || !instance || !here) return null;

  const joined = instance.participants.some((p) => p.user_id === me.id);
  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
    } catch {
      // The gateway event (or the next read) shows the real state; nothing to add here.
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside
      className="act-panel"
      aria-label={t('activities.panel.label', { name: instance.application_name })}
    >
      <div className="act-bar">
        <Icon name="sparkle" size={20} />
        <span className="act-bar-text">
          <strong>{instance.application_name}</strong>
          <span className="act-muted">
            {' '}
            · {t('activities.panel.playing', { n: instance.participants.length })}
          </span>
        </span>
        {joined ? (
          <>
            <Button
              size="sm"
              variant="ghost"
              loading={busy}
              onClick={() => void run(() => api.leave(instance.id))}
            >
              {t('activities.panel.leave')}
            </Button>
            {instance.started_by === me.id && (
              <Button
                size="sm"
                variant="ghost"
                loading={busy}
                onClick={() => void run(() => api.end(instance.id))}
              >
                {t('activities.panel.end')}
              </Button>
            )}
          </>
        ) : (
          <Button size="sm" loading={busy} onClick={() => void run(() => api.join(instance.id))}>
            {t('activities.panel.join')}
          </Button>
        )}
      </div>
      {joined && <ActivityFrame instance={instance} channelName={channelName} />}
    </aside>
  );
}
