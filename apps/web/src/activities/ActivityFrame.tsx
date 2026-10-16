// The iframe host of an Activity (DEV-ACT-004): a sandboxed frame on the Activity's own origin and the
// postMessage bridge to it. The sandbox keeps scripts, forms and popups, and `allow-same-origin` only means the
// Activity's own origin (never this app's: the frame is cross-origin). A strict `allow` list delegates nothing
// but full screen, and a message is believed only when it comes from this frame's window.
import { Button, Modal } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../i18n';
import { useUsers } from '../lounge/data';
import { activityOrigin, httpOAuthApi } from './api';
import { createActivityHost, type HostEnv } from './host';
import type { ActivityInstance } from './types';

type Ask = Parameters<HostEnv['confirm']>[0];

export function ActivityFrame({
  instance,
  channelName,
}: {
  instance: ActivityInstance;
  channelName: string;
}) {
  const t = useT();
  const users = useUsers();
  const frame = useRef<HTMLIFrameElement | null>(null);
  const [ask, setAsk] = useState<{ ask: Ask; answer: (ok: boolean) => void } | null>(null);
  const origin = useMemo(() => activityOrigin(instance.application_id), [instance.application_id]);

  // The host reads the latest instance and users through refs, so one host serves the frame's whole life.
  const latest = useRef({ instance, users, channelName });
  latest.current = { instance, users, channelName };

  const host = useMemo(
    () =>
      createActivityHost({
        origin,
        applicationId: instance.application_id,
        instance: () => latest.current.instance,
        channel: () => ({
          id: latest.current.instance.channel_id,
          guild_id: latest.current.instance.guild_id,
          name: latest.current.channelName,
        }),
        users: (ids) =>
          ids.flatMap((id) => {
            const u = latest.current.users[id];
            if (!u) return [];
            return [
              {
                id: u.id,
                username: u.username,
                global_name: u.displayName,
                avatar: null,
                avatar_url: u.avatarUrl ? new URL(u.avatarUrl, window.location.origin).href : null,
                bot: u.bot ?? false,
              },
            ];
          }),
        oauth: httpOAuthApi,
        confirm: (a) => new Promise<boolean>((answer) => setAsk({ ask: a, answer })),
        openLink: (url) => void window.open(url, '_blank', 'noopener,noreferrer'),
        post: (f) => frame.current?.contentWindow?.postMessage(f, origin),
      }),
    [origin, instance.application_id],
  );

  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (!frame.current || e.source !== frame.current.contentWindow) return;
      void host.handle(e.data, e.origin);
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [host]);

  // Tell the Activity when people join or leave.
  const roster = instance.participants.map((p) => p.user_id).join(',');
  useEffect(() => {
    host.participantsChanged();
  }, [host, roster]);

  const decide = (ok: boolean) => {
    ask?.answer(ok);
    setAsk(null);
  };

  return (
    <>
      <iframe
        ref={frame}
        className="act-frame"
        title={t('activities.frame.title', { name: instance.application_name })}
        src={`${origin}/`}
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        allow="fullscreen"
        referrerPolicy="no-referrer"
      />
      {ask && (
        <Modal
          open
          onClose={() => decide(false)}
          title={
            ask.ask.kind === 'authorize'
              ? t('activities.confirm.authorizeTitle', { name: instance.application_name })
              : t('activities.confirm.linkTitle', { name: instance.application_name })
          }
          description={
            ask.ask.kind === 'authorize'
              ? t('activities.confirm.authorizeBody', { scopes: ask.ask.scopes.join(', ') })
              : ask.ask.url
          }
          footer={
            <>
              <Button variant="ghost" onClick={() => decide(false)}>
                {t('activities.confirm.deny')}
              </Button>
              <Button onClick={() => decide(true)}>
                {ask.ask.kind === 'authorize'
                  ? t('activities.confirm.authorize')
                  : t('activities.confirm.openLink')}
              </Button>
            </>
          }
        >
          {ask.ask.kind === 'link' ? (
            <p className="act-muted">{t('activities.confirm.linkWarning')}</p>
          ) : null}
        </Modal>
      )}
    </>
  );
}
