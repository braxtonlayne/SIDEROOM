import {
  Avatar,
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  Modal,
  Tabs,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { cdnUrl, useChannels } from '../../../store';
import * as api from '../api';
import { useResource, type Viewer } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, NotAvailable, SectionHeader, formatDate, useAction } from '../ui';
import { ImageField } from './ImageField';

const origin = () => globalThis.location?.origin ?? '';

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function IntegrationsSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const tabs = [
    ...(viewer.can('MANAGE_WEBHOOKS')
      ? [
          {
            id: 'webhooks',
            label: t('settings.integrations.tabWebhooks'),
            content: <Webhooks guildId={guildId} />,
          },
        ]
      : []),
    ...(viewer.can('MANAGE_GUILD')
      ? [
          {
            id: 'templates',
            label: t('settings.integrations.tabTemplate'),
            content: <Templates guildId={guildId} viewer={viewer} />,
          },
        ]
      : []),
    {
      id: 'apps',
      label: t('settings.integrations.tabApps'),
      content: <NotAvailable>{t('settings.integrations.appsNotAvailable')}</NotAvailable>,
    },
  ];
  return (
    <>
      <SectionHeader
        title={t('settings.integrations.title')}
        description={t('settings.integrations.description')}
      />
      <Tabs label={t('settings.integrations.title')} items={tabs} />
    </>
  );
}

function Webhooks({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`webhooks:${guildId}`, () => api.listWebhooks(guildId));
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const [creating, setCreating] = useState(false);
  // The one place a webhook URL is ever shown: right after creating or resetting it (D-164).
  const [reveal, setReveal] = useState<{ hook: api.WebhookObject; reset: boolean } | null>(null);
  const [editing, setEditing] = useState<api.WebhookObject | null>(null);
  const [toReset, setToReset] = useState<api.WebhookObject | null>(null);
  const [toDelete, setToDelete] = useState<api.WebhookObject | null>(null);
  const [copied, setCopied] = useState(false);
  const action = useAction();

  const remove = async () => {
    if (!toDelete) return;
    const ok = await action.run(async () => {
      await api.deleteWebhook(toDelete.id);
      return true;
    });
    if (ok) {
      res.set((cur) => (cur ?? []).filter((w) => w.id !== toDelete.id));
      setToDelete(null);
    }
  };

  const resetUrl = async () => {
    if (!toReset) return;
    const hook = await action.run(() => api.resetWebhookToken(toReset.id));
    if (hook) {
      setReveal({ hook: { ...toReset, ...hook }, reset: true });
      setCopied(false);
      setToReset(null);
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <div className="ss-actions">
        <Button onClick={() => setCreating(true)}>
          {t('settings.integrations.createWebhook')}
        </Button>
      </div>
      <ActionStatus error={action.error} />
      {reveal?.hook.url && (
        <div className="ss-card" role="status">
          <h2>
            {reveal.reset
              ? t('settings.integrations.webhookNewUrl', { name: reveal.hook.name })
              : t('settings.integrations.webhookCreated', { name: reveal.hook.name })}
          </h2>
          <p className="ss-sub">
            {t('settings.integrations.copyUrlNow')}
            {reveal.reset ? t('settings.integrations.oldUrlDead') : ''}
          </p>
          <code className="ss-pre">{`${origin()}${reveal.hook.url}`}</code>
          <div className="ss-actions">
            <Button
              size="sm"
              onClick={() =>
                void copyText(`${origin()}${reveal.hook.url}`).then((ok) => {
                  setCopied(ok);
                  if (!ok) action.setError(t('settings.integrations.copyUrlBlocked'));
                })
              }
            >
              {copied
                ? t('settings.integrations.copied')
                : t('settings.integrations.copyWebhookUrl')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setReveal(null)}>
              {t('settings.integrations.dismiss')}
            </Button>
          </div>
        </div>
      )}
      <Async res={res} label={t('settings.integrations.loadingWebhooks')}>
        {(list) =>
          list.length === 0 ? (
            <EmptyState
              title={t('settings.integrations.noWebhooksTitle')}
              description={t('settings.integrations.noWebhooksDescription')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('settings.integrations.tabWebhooks')}>
              {list.map((w) => (
                <li key={w.id} className="ss-row">
                  <Avatar
                    name={w.name}
                    size={32}
                    src={cdnUrl('avatars', w.id, w.avatar, { size: 64 })}
                  />
                  <div className="ss-grow">
                    <div className="ss-name">{w.name}</div>
                    <div className="ss-meta">
                      #{channels.find((c) => c.id === w.channel_id)?.name ?? w.channel_id}
                      {w.user
                        ? t('settings.integrations.createdBy', { name: api.userName(w.user) })
                        : ''}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('settings.integrations.editWebhook', { name: w.name })}
                    onClick={() => setEditing(w)}
                  >
                    {t('settings.integrations.edit')}
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('settings.integrations.resetUrlFor', { name: w.name })}
                    title={t('settings.integrations.resetUrlTitle')}
                    onClick={() => setToReset(w)}
                  >
                    {t('settings.integrations.resetUrl')}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('settings.integrations.deleteWebhookNamed', { name: w.name })}
                    onClick={() => setToDelete(w)}
                  >
                    {t('settings.integrations.delete')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      {creating && (
        <WebhookDialog
          channels={channels.map((c) => ({ id: c.id, name: c.name ?? c.id }))}
          onClose={() => setCreating(false)}
          onSaved={async (w) => {
            setCreating(false);
            setReveal({ hook: w, reset: false });
            setCopied(false);
            await res.reload();
          }}
        />
      )}
      {editing && (
        <WebhookDialog
          channels={channels.map((c) => ({ id: c.id, name: c.name ?? c.id }))}
          webhook={editing}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await res.reload();
          }}
        />
      )}
      <ConfirmDialog
        open={!!toReset}
        danger
        title={t('settings.integrations.resetUrlFor', {
          name: toReset?.name ?? t('settings.integrations.webhookWord'),
        })}
        description={t('settings.integrations.resetDescription')}
        confirmLabel={t('settings.integrations.resetUrl')}
        loading={action.busy}
        onConfirm={() => void resetUrl()}
        onCancel={() => setToReset(null)}
      />
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={t('settings.integrations.deleteTitle', {
          name: toDelete?.name ?? t('settings.integrations.webhookWord'),
        })}
        description={t('settings.integrations.deleteWebhookDescription')}
        confirmLabel={t('settings.integrations.deleteWebhook')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

function WebhookDialog({
  channels,
  webhook,
  onClose,
  onSaved,
}: {
  channels: { id: string; name: string }[];
  /** Present when editing. */
  webhook?: api.WebhookObject;
  onClose: () => void;
  onSaved: (w: api.WebhookObject, created: boolean) => Promise<void>;
}) {
  const t = useT();
  const [name, setName] = useState(webhook?.name ?? '');
  const [channel, setChannel] = useState(webhook?.channel_id ?? channels[0]?.id ?? '');
  // The stored hash, a data URI picked now, or null.
  const [avatar, setAvatar] = useState<string | null>(webhook?.avatar ?? null);
  const action = useAction();
  const nameOk = name.trim().length >= 1 && name.trim().length <= 80;
  const submit = async () => {
    if (!nameOk || !channel) return;
    if (!webhook) {
      const w = await action.run(() =>
        api.createWebhook(channel, name.trim(), avatar ?? undefined),
      );
      if (w) await onSaved(w, true);
      return;
    }
    const body: Parameters<typeof api.modifyWebhook>[1] = {};
    if (name.trim() !== webhook.name) body.name = name.trim();
    if (channel !== webhook.channel_id) body.channel_id = channel;
    if (avatar !== (webhook.avatar ?? null)) body.avatar = avatar;
    if (Object.keys(body).length === 0) {
      onClose();
      return;
    }
    const w = await action.run(() => api.modifyWebhook(webhook.id, body));
    if (w) await onSaved(w, false);
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={
        webhook
          ? t('settings.integrations.editTitle', { name: webhook.name })
          : t('settings.integrations.createWebhook')
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.integrations.cancel')}
          </Button>
          <Button
            loading={action.busy}
            disabled={!nameOk || !channel}
            onClick={() => void submit()}
          >
            {webhook ? t('settings.integrations.save') : t('settings.integrations.create')}
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <ImageField
          label={t('settings.integrations.avatarLabel')}
          hint={t('settings.integrations.avatarHint')}
          ownerId={webhook?.id ?? ''}
          kind="avatars"
          shape="square"
          value={avatar}
          onChange={setAvatar}
        />
        <TextInput
          label={t('settings.integrations.webhookName')}
          value={name}
          maxLength={80}
          onChange={(e) => setName(e.target.value)}
        />
        <div>
          <label className="ss-field-label" htmlFor="ss-webhook-channel">
            {t('settings.integrations.channel')}
          </label>
          <select
            id="ss-webhook-channel"
            className="ss-select"
            value={channel}
            onChange={(e) => setChannel(e.target.value)}
          >
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                #{c.name}
              </option>
            ))}
          </select>
        </div>
      </div>
    </Modal>
  );
}

function Templates({ guildId }: { guildId: string; viewer: Viewer }) {
  const t = useT();
  const res = useResource(`templates:${guildId}`, () => api.listTemplates(guildId));
  const action = useAction();
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [toDelete, setToDelete] = useState<api.TemplateObject | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const create = async () => {
    const made = await action.run(
      () =>
        api.createTemplate(guildId, {
          name: name.trim(),
          ...(description.trim() ? { description: description.trim() } : {}),
        }),
      t('settings.integrations.templateCreated'),
    );
    if (made) {
      setName('');
      setDescription('');
      await res.reload();
    }
  };
  const sync = async (tpl: api.TemplateObject) => {
    const next = await action.run(
      () => api.syncTemplate(guildId, tpl.code),
      t('settings.integrations.templateSynced'),
    );
    if (next) await res.reload();
  };
  const remove = async () => {
    if (!toDelete) return;
    const ok = await action.run(async () => {
      await api.deleteTemplate(guildId, toDelete.code);
      return true;
    });
    if (ok) {
      setToDelete(null);
      await res.reload();
    }
  };
  const copy = async (code: string, text: string = code) => {
    if (await copyText(text)) {
      setCopied(text);
      setTimeout(() => setCopied((c) => (c === text ? null : c)), 2000);
    } else action.setError(t('settings.integrations.copyTextBlocked'));
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <p className="ss-sub">{t('settings.integrations.templateIntro')}</p>
      <ActionStatus error={action.error} done={action.done} />
      <Async res={res} label={t('settings.integrations.loadingTemplates')}>
        {(list) => (
          <>
            {list.map((tpl) => (
              <div key={tpl.code} className="ss-card">
                <h2>{tpl.name}</h2>
                {tpl.description && <p className="ss-sub">{tpl.description}</p>}
                <p className="ss-meta">
                  {t('settings.integrations.templateUsed', {
                    count: String(tpl.usage_count),
                    date: formatDate(tpl.updated_at),
                  })}{' '}
                  {tpl.is_dirty && (
                    <Badge variant="warning">{t('settings.integrations.outOfDate')}</Badge>
                  )}
                </p>
                <code
                  className="ss-pre"
                  aria-label={t('settings.integrations.codeFor', { name: tpl.name })}
                >
                  {tpl.code}
                </code>
                <div className="ss-actions">
                  <Button size="sm" onClick={() => void copy(tpl.code)}>
                    {copied === tpl.code
                      ? t('settings.integrations.copied')
                      : t('settings.integrations.copyCode')}
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('settings.integrations.copyLinkFor', { name: tpl.name })}
                    onClick={() => void copy(tpl.code, `${origin()}/template/${tpl.code}`)}
                  >
                    {copied === `${origin()}/template/${tpl.code}`
                      ? t('settings.integrations.copied')
                      : t('settings.integrations.copyLink')}
                  </Button>
                  <a
                    className="ss-link"
                    href={`/template/${tpl.code}`}
                    target="_blank"
                    rel="noreferrer"
                    aria-label={t('settings.integrations.previewFor', { name: tpl.name })}
                  >
                    {t('settings.integrations.preview')}
                  </a>
                  <Button
                    size="sm"
                    variant="secondary"
                    loading={action.busy}
                    aria-label={t('settings.integrations.syncFor', { name: tpl.name })}
                    onClick={() => void sync(tpl)}
                  >
                    {t('settings.integrations.sync')}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('settings.integrations.deleteTemplateNamed', { name: tpl.name })}
                    onClick={() => setToDelete(tpl)}
                  >
                    {t('settings.integrations.delete')}
                  </Button>
                </div>
              </div>
            ))}
            {list.length === 0 && (
              <form
                className="ss-card"
                onSubmit={(e) => {
                  e.preventDefault();
                  void create();
                }}
              >
                <h2>{t('settings.integrations.createTemplate')}</h2>
                <TextInput
                  label={t('settings.integrations.templateName')}
                  value={name}
                  maxLength={100}
                  onChange={(e) => setName(e.target.value)}
                />
                <TextArea
                  label={t('settings.integrations.descriptionLabel')}
                  value={description}
                  maxLength={120}
                  onChange={(e) => setDescription(e.target.value)}
                />
                <div>
                  <Button type="submit" disabled={!name.trim()} loading={action.busy}>
                    {t('settings.integrations.generate')}
                  </Button>
                </div>
              </form>
            )}
          </>
        )}
      </Async>
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={t('settings.integrations.deleteTitle', {
          name: toDelete?.name ?? t('settings.integrations.templateWord'),
        })}
        description={t('settings.integrations.deleteTemplateDescription')}
        confirmLabel={t('settings.integrations.deleteTemplate')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}
