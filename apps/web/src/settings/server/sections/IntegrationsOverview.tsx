import { Avatar, Button, ConfirmDialog, EmptyState } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { cdnUrl, useChannels } from '../../../store';
import * as api from '../api';
import { useResource, type Viewer } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';
import { BotCommandPermissions } from './BotCommandPermissions';

/** Discord's webhook types: 1 incoming, 2 channel follower. */
const FOLLOWER = 2;

/**
 * Integrations overview (LNG-SSET-044): how many webhooks and followed channels the server has, and the applications
 * installed in it (`GET /guilds/{id}/integrations`, J-05). A server manager opens an application's command
 * permissions (J-10, LNG-SSET-047) or removes it: the bot leaves and its role goes with it.
 */
export function IntegrationsOverview({ guildId, viewer }: { guildId: string; viewer: Viewer }) {
  const t = useT();
  const hooks = useResource(`integrations-hooks:${guildId}`, () => api.listWebhooks(guildId));
  const canManage = viewer.can('MANAGE_GUILD');
  const apps = useResource(`integrations-apps:${guildId}`, () =>
    canManage ? api.listIntegrations(guildId) : Promise.resolve([]),
  );
  const [toRemove, setToRemove] = useState<api.IntegrationObject | null>(null);
  const [editing, setEditing] = useState<api.IntegrationObject | null>(null);
  const action = useAction();
  const canWebhooks = viewer.can('MANAGE_WEBHOOKS');

  const remove = async () => {
    if (!toRemove) return;
    const ok = await action.run(async () => {
      await api.removeIntegration(guildId, toRemove.id);
      return true;
    });
    if (ok) {
      apps.set((cur) => (cur ?? []).filter((a) => a.id !== toRemove.id));
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
      {canManage && (
        <section aria-label={t('settings.integrationsOverview.botsTitle')}>
          <h2 className="ss-sub">{t('settings.integrationsOverview.botsTitle')}</h2>
          <Async res={apps} label={t('settings.integrationsOverview.loadingBots')}>
            {(list) =>
              list.length === 0 ? (
                <EmptyState
                  title={t('settings.integrationsOverview.noBots')}
                  description={t('settings.integrationsOverview.noAppsHint')}
                />
              ) : (
                <ul className="ss-list" aria-label={t('settings.integrationsOverview.botsTitle')}>
                  {list.map((a) => (
                    <li key={a.id} className="ss-row">
                      <Avatar
                        name={a.name}
                        size={32}
                        src={cdnUrl('avatars', a.application.bot.id, a.application.icon, {
                          size: 64,
                        })}
                      />
                      <div className="ss-grow">
                        <div className="ss-name">{a.name}</div>
                        <div className="ss-meta">
                          {t('settings.integrationsOverview.addedBy', {
                            name: api.userName(a.user),
                          })}
                        </div>
                      </div>
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('settings.integrationsOverview.commandsNamed', {
                          name: a.name,
                        })}
                        onClick={() => setEditing(a)}
                      >
                        {t('settings.integrationsOverview.commands')}
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('settings.integrationsOverview.removeNamed', {
                          name: a.name,
                        })}
                        onClick={() => setToRemove(a)}
                      >
                        {t('settings.integrationsOverview.remove')}
                      </Button>
                    </li>
                  ))}
                </ul>
              )
            }
          </Async>
        </section>
      )}
      <ConfirmDialog
        open={!!toRemove}
        danger
        title={t('settings.integrationsOverview.removeTitle', { name: toRemove?.name ?? '' })}
        description={t('settings.integrationsOverview.removeBody')}
        confirmLabel={t('settings.integrationsOverview.remove')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToRemove(null)}
      />
      {editing && (
        <BotCommandPermissions
          guildId={guildId}
          appId={editing.id}
          appName={editing.name}
          onClose={() => setEditing(null)}
        />
      )}
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
