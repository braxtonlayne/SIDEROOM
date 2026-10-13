import {
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
import { useChannels } from '../../../store';
import * as api from '../api';
import { useResource, type Viewer } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, NotAvailable, SectionHeader, formatDate, useAction } from '../ui';

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
  const tabs = [
    ...(viewer.can('MANAGE_WEBHOOKS')
      ? [{ id: 'webhooks', label: 'Webhooks', content: <Webhooks guildId={guildId} /> }]
      : []),
    ...(viewer.can('MANAGE_GUILD')
      ? [
          {
            id: 'templates',
            label: 'Server Template',
            content: <Templates guildId={guildId} viewer={viewer} />,
          },
        ]
      : []),
    {
      id: 'apps',
      label: 'Bots and Apps',
      content: (
        <NotAvailable>
          Bots, followed channels and the App Directory ship with the developer platform.
        </NotAvailable>
      ),
    },
  ];
  return (
    <>
      <SectionHeader
        title="Integrations"
        description="Connect webhooks and share a template of this server."
      />
      <Tabs label="Integrations" items={tabs} />
    </>
  );
}

function Webhooks({ guildId }: { guildId: string }) {
  const res = useResource(`webhooks:${guildId}`, () => api.listWebhooks(guildId));
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const [creating, setCreating] = useState(false);
  // The one place a webhook URL is ever shown: right after creating or resetting it (D-164).
  const [reveal, setReveal] = useState<{ hook: api.WebhookObject; reset: boolean } | null>(null);
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
        <Button onClick={() => setCreating(true)}>Create webhook</Button>
      </div>
      <ActionStatus error={action.error} />
      {reveal?.hook.url && (
        <div className="ss-card" role="status">
          <h2>
            {reveal.reset ? 'New webhook URL' : 'Webhook created'}: {reveal.hook.name}
          </h2>
          <p className="ss-sub">
            Copy the URL now. It contains the secret token and is only shown once.
            {reveal.reset ? ' The old URL no longer works.' : ''}
          </p>
          <code className="ss-pre">{`${origin()}${reveal.hook.url}`}</code>
          <div className="ss-actions">
            <Button
              size="sm"
              onClick={() =>
                void copyText(`${origin()}${reveal.hook.url}`).then((ok) => {
                  setCopied(ok);
                  if (!ok)
                    action.setError(
                      'Copying is blocked by the browser. Select the URL and copy it by hand.',
                    );
                })
              }
            >
              {copied ? 'Copied!' : 'Copy webhook URL'}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setReveal(null)}>
              Dismiss
            </Button>
          </div>
        </div>
      )}
      <Async res={res} label="Loading webhooks">
        {(list) =>
          list.length === 0 ? (
            <EmptyState
              title="No webhooks"
              description="Create a webhook to post into a channel from another service."
            />
          ) : (
            <ul className="ss-list" aria-label="Webhooks">
              {list.map((w) => (
                <li key={w.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">{w.name}</div>
                    <div className="ss-meta">
                      #{channels.find((c) => c.id === w.channel_id)?.name ?? w.channel_id}
                      {w.user ? ` - created by ${api.userName(w.user)}` : ''}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={`Reset URL for ${w.name}`}
                    title="URLs are only shown once. Reset issues a new one."
                    onClick={() => setToReset(w)}
                  >
                    Reset URL
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Delete webhook ${w.name}`}
                    onClick={() => setToDelete(w)}
                  >
                    Delete
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      {creating && (
        <CreateWebhook
          channels={channels.map((c) => ({ id: c.id, name: c.name ?? c.id }))}
          onClose={() => setCreating(false)}
          onCreated={async (w) => {
            setCreating(false);
            setReveal({ hook: w, reset: false });
            setCopied(false);
            await res.reload();
          }}
        />
      )}
      <ConfirmDialog
        open={!!toReset}
        danger
        title={`Reset URL for ${toReset?.name ?? 'webhook'}`}
        description="A new URL is created and shown once. The current URL stops working immediately."
        confirmLabel="Reset URL"
        loading={action.busy}
        onConfirm={() => void resetUrl()}
        onCancel={() => setToReset(null)}
      />
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={`Delete ${toDelete?.name ?? 'webhook'}`}
        description="Anything posting with this webhook stops working."
        confirmLabel="Delete webhook"
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

function CreateWebhook({
  channels,
  onClose,
  onCreated,
}: {
  channels: { id: string; name: string }[];
  onClose: () => void;
  onCreated: (w: api.WebhookObject) => Promise<void>;
}) {
  const [name, setName] = useState('');
  const [channel, setChannel] = useState(channels[0]?.id ?? '');
  const action = useAction();
  const nameOk = name.trim().length >= 1 && name.trim().length <= 80;
  const submit = async () => {
    if (!nameOk || !channel) return;
    const w = await action.run(() => api.createWebhook(channel, name.trim()));
    if (w) await onCreated(w);
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title="Create webhook"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            loading={action.busy}
            disabled={!nameOk || !channel}
            onClick={() => void submit()}
          >
            Create
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <TextInput
          label="Webhook name"
          value={name}
          maxLength={80}
          onChange={(e) => setName(e.target.value)}
        />
        <div>
          <label className="ss-field-label" htmlFor="ss-webhook-channel">
            Channel
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
  const res = useResource(`templates:${guildId}`, () => api.listTemplates(guildId));
  const action = useAction();
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [toDelete, setToDelete] = useState<api.TemplateObject | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const create = async () => {
    const t = await action.run(
      () =>
        api.createTemplate(guildId, {
          name: name.trim(),
          ...(description.trim() ? { description: description.trim() } : {}),
        }),
      'Template created.',
    );
    if (t) {
      setName('');
      setDescription('');
      await res.reload();
    }
  };
  const sync = async (t: api.TemplateObject) => {
    const next = await action.run(
      () => api.syncTemplate(guildId, t.code),
      'Template synced with the server.',
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
  const copy = async (code: string) => {
    if (await copyText(code)) {
      setCopied(code);
      setTimeout(() => setCopied((c) => (c === code ? null : c)), 2000);
    } else
      action.setError('Copying is blocked by the browser. Select the code and copy it by hand.');
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <p className="ss-sub">
        A template copies this server&apos;s channels, roles and permissions (not its messages or
        members) so others can start a server from it.
      </p>
      <ActionStatus error={action.error} done={action.done} />
      <Async res={res} label="Loading templates">
        {(list) => (
          <>
            {list.map((t) => (
              <div key={t.code} className="ss-card">
                <h2>{t.name}</h2>
                {t.description && <p className="ss-sub">{t.description}</p>}
                <p className="ss-meta">
                  Used {t.usage_count} times - updated {formatDate(t.updated_at)}{' '}
                  {t.is_dirty && <Badge variant="warning">Out of date</Badge>}
                </p>
                <code className="ss-pre" aria-label={`Template code for ${t.name}`}>
                  {t.code}
                </code>
                <div className="ss-actions">
                  <Button size="sm" onClick={() => void copy(t.code)}>
                    {copied === t.code ? 'Copied!' : 'Copy code'}
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    loading={action.busy}
                    aria-label={`Sync template ${t.name}`}
                    onClick={() => void sync(t)}
                  >
                    Sync
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Delete template ${t.name}`}
                    onClick={() => setToDelete(t)}
                  >
                    Delete
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
                <h2>Create a template</h2>
                <TextInput
                  label="Template name"
                  value={name}
                  maxLength={100}
                  onChange={(e) => setName(e.target.value)}
                />
                <TextArea
                  label="Description"
                  value={description}
                  maxLength={120}
                  onChange={(e) => setDescription(e.target.value)}
                />
                <div>
                  <Button type="submit" disabled={!name.trim()} loading={action.busy}>
                    Generate template
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
        title={`Delete ${toDelete?.name ?? 'template'}`}
        description="The template code stops working."
        confirmLabel="Delete template"
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}
