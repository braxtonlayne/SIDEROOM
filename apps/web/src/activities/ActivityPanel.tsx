// The Activity of a channel, shown above its content (DEV-ACT-004, J-15): a bar with Join / Leave / End and,
// once you joined, the slot the frame is laid over. The frame itself is rendered once by `ActivityDock` in the
// Lounge shell, so it survives navigating away from this channel (it docks as a small window) and coming back.
import { Button, Icon } from '@sideroom/ui';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { useCurrentUser } from '../lounge/data';
import { httpActivityApi, type ActivityApi } from './api';
import { useChannelActivity } from './hooks';
import { useActivitySession } from './session';
import './activities.css';

/** The reserved area of the frame: publishes its on-screen rectangle to the session store. */
function ActivitySlot({ channelId }: { channelId: string }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const setSlot = useActivitySession((s) => s.setSlot);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setSlot(channelId, { left: r.left, top: r.top, width: r.width, height: r.height });
    };
    measure();
    const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    ro?.observe(el);
    window.addEventListener('resize', measure);
    // Any scrolling ancestor moves the slot.
    window.addEventListener('scroll', measure, true);
    return () => {
      ro?.disconnect();
      window.removeEventListener('resize', measure);
      window.removeEventListener('scroll', measure, true);
      setSlot(channelId, null);
    };
  }, [channelId, setSlot]);
  return <div ref={ref} className="act-slot" data-testid="activity-slot" />;
}

export function ActivityPanel({
  channelId,
  here,
  api = httpActivityApi,
}: {
  channelId: string;
  /** You may take part from this view: in the call for a voice channel, always for a text channel. */
  here: boolean;
  api?: ActivityApi;
}) {
  const t = useT();
  const me = useCurrentUser();
  const { enabled, instance } = useChannelActivity(channelId, api);
  const adopt = useActivitySession((s) => s.adopt);
  const [busy, setBusy] = useState(false);

  // Keep the shell's dock informed about this channel's instance.
  useEffect(() => {
    if (enabled) adopt(channelId, instance, me.id);
  }, [enabled, instance, channelId, me.id, adopt]);

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
          {instance.development && (
            <span className="act-badge">{t('activities.panel.development')}</span>
          )}
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
      {joined && <ActivitySlot channelId={channelId} />}
    </aside>
  );
}
