/**
 * The Invites, Integrations and Features tabs of the channel editor (LNG-CHN-017). Invites and webhooks are the
 * channel's own; Features holds the per-channel settings the server accepts for threads. Per-channel switches
 * for feature modules (docs/11) have no endpoint yet: the registry only has `/admin/features`, so this tab does
 * not pretend to offer them.
 */
import { Button, ConfirmDialog, EmptyState, Modal, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { expiryLabel, inviteUrl } from '../../../lounge/inviteFlow';
import * as api from '../api';
import { useResource } from '../hooks';
import { ActionStatus, Async, formatDate, useAction } from '../ui';

const origin = () => globalThis.location?.origin ?? '';

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Active invites to this channel, with copy, revoke and create. */
export function ChannelInvites({ channel }: { channel: api.ChannelObject }) {
  const res = useResource(`channel-invites:${channel.id}`, () =>
    api.listChannelInvites(channel.id),
  );
  const action = useAction();
  const [copied, setCopied] = useState<string | null>(null);

  const create = async () => {
    const inv = await action.run(() => api.createInvite(channel.id, {}));
    if (inv) res.set((cur) => [inv, ...(cur ?? [])]);
  };
  const revoke = async (code: string) => {
    const ok = await action.run(async () => {
      await api.revokeInvite(code);
      return true;
    });
    if (ok) res.set((cur) => (cur ?? []).filter((i) => i.code !== code));
  };
  const copy = async (code: string) => {
    if (await copyText(inviteUrl(code))) {
      setCopied(code);
      setTimeout(() => setCopied((c) => (c === code ? null : c)), 2000);
    } else {
      action.setError('Copying is blocked by the browser. Select the link and copy it by hand.');
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <div className="ss-actions">
        <Button onClick={() => void create()} loading={action.busy}>
          Create invite
        </Button>
      </div>
      <ActionStatus error={action.error} />
      <Async res={res} label="Loading invites">
        {(invites) =>
          invites.length === 0 ? (
            <EmptyState
              title="No active invites"
              description="Create an invite to this channel to bring people in."
            />
          ) : (
            <ul className="ss-list" aria-label="Invites to this channel">
              {invites.map((i) => (
                <li key={i.code} className="ss-row">
                  <span>
                    <code>{i.code}</code>
                    <span className="ss-meta">
                      {' '}
                      {api.userName(i.inviter as api.UserRef | undefined, 'Unknown')} ·{' '}
                      {formatDate(i.created_at)} · {i.uses}
                      {i.max_uses ? ` / ${i.max_uses}` : ''} uses · {expiryLabel(i.expires_at)}
                    </span>
                  </span>
                  <span className="ss-actions">
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`Copy link for ${i.code}`}
                      onClick={() => void copy(i.code)}
                    >
                      {copied === i.code ? 'Copied!' : 'Copy link'}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={`Revoke invite ${i.code}`}
                      onClick={() => void revoke(i.code)}
                    >
                      Revoke
                    </Button>
                  </span>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
    </div>
  );
}

/** Webhooks posting into this channel. A URL is shown once, right after creating or resetting it (D-164). */
export function ChannelIntegrations({
  guildId,
  channel,
}: {
  guildId: string;
  channel: api.ChannelObject;
}) {
  const res = useResource(`channel-webhooks:${channel.id}`, async () =>
    (await api.listWebhooks(guildId)).filter((w) => w.channel_id === channel.id),
  );
  const action = useAction();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [reveal, setReveal] = useState<{ hook: api.WebhookObject; reset: boolean } | null>(null);
  const [copied, setCopied] = useState(false);
  const [toDelete, setToDelete] = useState<api.WebhookObject | null>(null);
  const [toReset, setToReset] = useState<api.WebhookObject | null>(null);

  const create = async () => {
    const hook = await action.run(() => api.createWebhook(channel.id, name.trim()));
    if (hook) {
      res.set((cur) => [...(cur ?? []), hook]);
      setReveal({ hook, reset: false });
      setCopied(false);
      setCreating(false);
      setName('');
    }
  };
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
          <h3>
            {reveal.reset ? 'New webhook URL' : 'Webhook created'}: {reveal.hook.name}
          </h3>
          <p className="ss-sub">
            Copy the URL now. It contains the secret token and is only shown once.
          </p>
          <code className="ss-pre">{`${origin()}${reveal.hook.url}`}</code>
          <div className="ss-actions">
            <Button
              size="sm"
              onClick={() =>
                void copyText(`${origin()}${reveal.hook.url}`).then((ok) => {
                  setCopied(ok);
                  if (!ok) action.setError('Copying is blocked by the browser.');
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
        {(hooks) =>
          hooks.length === 0 ? (
            <EmptyState
              title="No webhooks in this channel"
              description="Create a webhook to post here from another service."
            />
          ) : (
            <ul className="ss-list" aria-label="Webhooks in this channel">
              {hooks.map((w) => (
                <li key={w.id} className="ss-row">
                  <span>
                    <strong>{w.name}</strong>
                    <span className="ss-meta"> created by {api.userName(w.user, 'someone')}</span>
                  </span>
                  <span className="ss-actions">
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`Reset URL of ${w.name}`}
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
                  </span>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <p className="ss-na">
        Followed announcement channels will be listed here once channel following is available.
      </p>
      <Modal
        open={creating}
        onClose={() => setCreating(false)}
        size="sm"
        title="Create webhook"
        footer={
          <>
            <Button variant="ghost" onClick={() => setCreating(false)}>
              Cancel
            </Button>
            <Button loading={action.busy} disabled={!name.trim()} onClick={() => void create()}>
              Create
            </Button>
          </>
        }
      >
        <TextInput
          label="Webhook name"
          value={name}
          maxLength={80}
          autoFocus
          onChange={(e) => setName(e.target.value)}
        />
      </Modal>
      <ConfirmDialog
        open={!!toReset}
        title={`Reset URL for ${toReset?.name ?? 'webhook'}`}
        description="The old URL stops working. You will see the new one once."
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

/** Discord's auto-archive choices, in minutes. */
export const ARCHIVE_MINUTES = [
  { value: 60, label: '1 hour' },
  { value: 1440, label: '24 hours' },
  { value: 4320, label: '3 days' },
  { value: 10080, label: '1 week' },
] as const;
export const THREAD_SLOWMODE = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600];
const slow = (s: number) =>
  s === 0 ? 'Off' : s < 60 ? `${s}s` : s < 3600 ? `${s / 60}m` : `${s / 3600}h`;

/** Thread defaults for a text or announcement channel. Saved as soon as a value changes. */
export function ChannelFeatures({
  channel,
  onChanged,
}: {
  channel: api.ChannelObject;
  onChanged: () => Promise<void>;
}) {
  const action = useAction();
  const hostsThreads = channel.type === 0 || channel.type === 5;
  const [archive, setArchive] = useState(channel.default_auto_archive_duration ?? 1440);
  const [slowmode, setSlowmode] = useState(channel.default_thread_rate_limit_per_user ?? 0);

  const save = async (body: api.ChannelBody, done: string) => {
    const ok = await action.run(async () => {
      await api.modifyChannel(channel.id, body);
      return true;
    }, done);
    if (ok) await onChanged();
  };

  if (!hostsThreads)
    return <p className="ss-na">This kind of channel has no extra feature settings.</p>;
  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <h3 className="ss-sub-title">Threads</h3>
      <div>
        <label className="ss-field-label" htmlFor="ss-thread-archive">
          Hide threads after inactivity
        </label>
        <select
          id="ss-thread-archive"
          className="ss-select"
          value={archive}
          disabled={action.busy}
          onChange={(e) => {
            const v = Number(e.target.value);
            setArchive(v);
            void save({ default_auto_archive_duration: v }, 'Saved.');
          }}
        >
          {ARCHIVE_MINUTES.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
      <div>
        <label className="ss-field-label" htmlFor="ss-thread-slow">
          Slowmode for new threads
        </label>
        <select
          id="ss-thread-slow"
          className="ss-select"
          value={slowmode}
          disabled={action.busy}
          onChange={(e) => {
            const v = Number(e.target.value);
            setSlowmode(v);
            void save({ default_thread_rate_limit_per_user: v }, 'Saved.');
          }}
        >
          {THREAD_SLOWMODE.map((s) => (
            <option key={s} value={s}>
              {slow(s)}
            </option>
          ))}
        </select>
      </div>
      <p className="ss-na">
        Switching a channel between text and announcement, and per-channel feature modules, are not
        available yet.
      </p>
    </div>
  );
}
