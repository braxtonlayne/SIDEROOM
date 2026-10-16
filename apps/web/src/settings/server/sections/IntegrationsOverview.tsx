import { Avatar, Button, ConfirmDialog, EmptyState } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { cdnUrl, useChannels } from '../../../store';
import * as api from '../api';
import { useResource, type Viewer } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';

/** Discord's webhook types: 1 incoming, 2 channel follower. */
const FOLLOWER = 2;

/** Members of a guild that are bots, from the first 1000 members (the largest page the API serves). */
const listBots = async (guildId: string) =>
  (await api.listMembers(guildId, { limit: 1000 })).filter(
    (m) => (m.user as api.UserRef & { bot?: boolean }).bot,
  );

/**
 * Integrations overview (LNG-SSET-044): how many webhooks and followed channels the server has, and the bots in it.
 * Removing a bot kicks its member (the bot's app stays installed on the developer's side).
 */
export function IntegrationsOverview({ guildId, viewer }: { guildId: string; viewer: Viewer }) {
  const t = useT();
  const hooks = useResource(`integrations-hooks:${guildId}`, () => api.listWebhooks(guildId));
  const bots = useResource(`integrations-bots:${guildId}`, () => listBots(guildId));
  const [toRemove, setToRemove] = useState<api.MemberObject | null>(null);
  const action = useAction();
  const canWebhooks = viewer.can('MANAGE_WEBHOOKS');
  const canKick = viewer.can('KICK_MEMBERS');

  const remove = async () => {
    if (!toRemove) return;
    const ok = await action.run(async () => {
      await api.kickMember(guildId, toRemove.user.id);
      return true;
    });
    if (ok) {
      bots.set((cur) => (cur ?? []).filter((m) => m.user.id !== toRemove.user.id));
      setToRemove(null);
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <ActionStatus error={action.error} />
      {canWebhooks && (
        <Async res={hooks} label={t('settings.integrationsOverview.loading')}>
          {(list) => (
            <ul className="ss-list" aria-label={t('settings.integrationsOverview.counts')}>
              <li>
                {t('settings.integrationsOverview.webhooks', {
                  count: list.filter((w) => w.type !== FOLLOWER).length,
                })}
              </li>
              <li>
                {t('settings.integrationsOverview.followed', {
                  count: list.filter((w) => w.type === FOLLOWER).length,
                })}
              </li>
            </ul>
          )}
        </Async>
      )}
      <section aria-label={t('settings.integrationsOverview.botsTitle')}>
        <h2 className="ss-sub">{t('settings.integrationsOverview.botsTitle')}</h2>
        <Async res={bots} label={t('settings.integrationsOverview.loadingBots')}>
          {(list) =>
            list.length === 0 ? (
              <EmptyState
                title={t('settings.integrationsOverview.noBots')}
                description={t('settings.integrationsOverview.noBotsHint')}
              />
            ) : (
              <ul className="ss-list" aria-label={t('settings.integrationsOverview.botsTitle')}>
                {list.map((m) => {
                  const name = api.userName(m.user);
                  return (
                    <li key={m.user.id} className="ss-row">
                      <Avatar
                        name={name}
                        size={32}
                        src={cdnUrl('avatars', m.user.id, (m.user as { avatar?: string }).avatar, {
                          size: 64,
                        })}
                      />
                      <div className="ss-grow">
                        <div className="ss-name">{name}</div>
                      </div>
                      {canKick && (
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('settings.integrationsOverview.removeNamed', { name })}
                          onClick={() => setToRemove(m)}
                        >
                          {t('settings.integrationsOverview.remove')}
                        </Button>
                      )}
                    </li>
                  );
                })}
              </ul>
            )
          }
        </Async>
      </section>
      <ConfirmDialog
        open={!!toRemove}
        danger
        title={t('settings.integrationsOverview.removeTitle', {
          name: toRemove ? api.userName(toRemove.user) : '',
        })}
        description={t('settings.integrationsOverview.removeBody')}
        confirmLabel={t('settings.integrationsOverview.remove')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToRemove(null)}
      />
    </div>
  );
}

/** Followed channels (LNG-SSET-046): announcement channels this server follows, with Unfollow. */
export function FollowedChannels({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`integrations-followed:${guildId}`, () => api.listWebhooks(guildId));
  const channels = useChannels(guildId);
  const [toRemove, setToRemove] = useState<api.WebhookObject | null>(null);
  const action = useAction();

  const unfollow = async () => {
    if (!toRemove) return;
    const ok = await action.run(async () => {
      await api.deleteWebhook(toRemove.id);
      return true;
    });
    if (ok) {
      res.set((cur) => (cur ?? []).filter((w) => w.id !== toRemove.id));
      setToRemove(null);
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <ActionStatus error={action.error} />
      <Async res={res} label={t('settings.integrationsOverview.loading')}>
        {(all) => {
          const list = all.filter((w) => w.type === FOLLOWER);
          return list.length === 0 ? (
            <EmptyState
              title={t('settings.integrationsOverview.noFollowed')}
              description={t('settings.integrationsOverview.noFollowedHint')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('settings.integrationsOverview.tabFollowed')}>
              {list.map((w) => (
                <li key={w.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">{w.name}</div>
                    <div className="ss-meta">
                      {t('settings.integrationsOverview.postsTo', {
                        name: channels.find((c) => c.id === w.channel_id)?.name ?? w.channel_id,
                      })}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('settings.integrationsOverview.unfollowNamed', { name: w.name })}
                    onClick={() => setToRemove(w)}
                  >
                    {t('settings.integrationsOverview.unfollow')}
                  </Button>
                </li>
              ))}
            </ul>
          );
        }}
      </Async>
      <ConfirmDialog
        open={!!toRemove}
        danger
        title={t('settings.integrationsOverview.unfollowTitle', { name: toRemove?.name ?? '' })}
        description={t('settings.integrationsOverview.unfollowBody')}
        confirmLabel={t('settings.integrationsOverview.unfollow')}
        loading={action.busy}
        onConfirm={() => void unfollow()}
        onCancel={() => setToRemove(null)}
      />
    </div>
  );
}
