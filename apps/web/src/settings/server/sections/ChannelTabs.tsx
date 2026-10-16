/**
 * The Invites, Integrations and Features tabs of the channel editor (LNG-CHN-017). Invites and webhooks are the
 * channel's own; Features holds the per-channel settings the server accepts for threads. Per-channel switches
 * for feature modules (docs/11) have no endpoint yet: the registry only has `/admin/features`, so this tab does
 * not pretend to offer them.
 */
import { Button, ConfirmDialog, EmptyState, Modal, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { t as translate, useT } from '../../../i18n';
import { MAX_STATUS, setChannelStatus, useChannelStatus } from '../../../voice/channelStatus';
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
  const t = useT();
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
      action.setError(t('settings.channelTabs.copyBlocked'));
    }
  };

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <div className="ss-actions">
        <Button onClick={() => void create()} loading={action.busy}>
          {t('settings.channelTabs.createInvite')}
        </Button>
      </div>
      <ActionStatus error={action.error} />
      <Async res={res} label={t('settings.channelTabs.loadingInvites')}>
        {(invites) =>
          invites.length === 0 ? (
            <EmptyState
              title={t('settings.channelTabs.noInvitesTitle')}
              description={t('settings.channelTabs.noInvitesDescription')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('settings.channelTabs.invitesLabel')}>
              {invites.map((i) => (
                <li key={i.code} className="ss-row">
                  <span>
                    <code>{i.code}</code>
                    <span className="ss-meta">
                      {' '}
                      {t('settings.channelTabs.inviteMeta', {
                        inviter: api.userName(
                          i.inviter as api.UserRef | undefined,
                          t('settings.channelTabs.unknown'),
                        ),
                        date: formatDate(i.created_at),
                        uses: `${i.uses}${i.max_uses ? ` / ${i.max_uses}` : ''}`,
                        expiry: expiryLabel(i.expires_at),
                      })}
                    </span>
                  </span>
                  <span className="ss-actions">
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('settings.channelTabs.copyFor', { code: i.code })}
                      onClick={() => void copy(i.code)}
                    >
                      {copied === i.code
                        ? t('settings.channelTabs.copied')
                        : t('settings.channelTabs.copyLink')}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={t('settings.channelTabs.revokeFor', { code: i.code })}
                      onClick={() => void revoke(i.code)}
                    >
                      {t('settings.channelTabs.revoke')}
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
  const t = useT();
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
        <Button onClick={() => setCreating(true)}>{t('settings.channelTabs.createWebhook')}</Button>
      </div>
      <ActionStatus error={action.error} />
      {reveal?.hook.url && (
        <div className="ss-card" role="status">
          <h3>
            {reveal.reset
              ? t('settings.channelTabs.webhookNewUrl', { name: reveal.hook.name })
              : t('settings.channelTabs.webhookCreated', { name: reveal.hook.name })}
          </h3>
          <p className="ss-sub">{t('settings.channelTabs.copyUrlNow')}</p>
          <code className="ss-pre">{`${origin()}${reveal.hook.url}`}</code>
          <div className="ss-actions">
            <Button
              size="sm"
              onClick={() =>
                void copyText(`${origin()}${reveal.hook.url}`).then((ok) => {
                  setCopied(ok);
                  if (!ok) action.setError(t('settings.channelTabs.copyBlockedShort'));
                })
              }
            >
              {copied ? t('settings.channelTabs.copied') : t('settings.channelTabs.copyWebhookUrl')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setReveal(null)}>
              {t('settings.channelTabs.dismiss')}
            </Button>
          </div>
        </div>
      )}
      <Async res={res} label={t('settings.channelTabs.loadingWebhooks')}>
        {(all) => {
          // Channel followers (type 2) are listed below; they have no URL to reset.
          const hooks = all.filter((w) => w.type !== 2);
          return hooks.length === 0 ? (
            <EmptyState
              title={t('settings.channelTabs.noWebhooksTitle')}
              description={t('settings.channelTabs.noWebhooksDescription')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('settings.channelTabs.webhooksLabel')}>
              {hooks.map((w) => (
                <li key={w.id} className="ss-row">
                  <span>
                    <strong>{w.name}</strong>
                    <span className="ss-meta">
                      {' '}
                      {t('settings.channelTabs.createdBy', {
                        name: api.userName(w.user, t('settings.channelTabs.someone')),
                      })}
                    </span>
                  </span>
                  <span className="ss-actions">
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('settings.channelTabs.resetUrlOf', { name: w.name })}
                      onClick={() => setToReset(w)}
                    >
                      {t('settings.channelTabs.resetUrl')}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={t('settings.channelTabs.deleteWebhookNamed', { name: w.name })}
                      onClick={() => setToDelete(w)}
                    >
                      {t('settings.channelTabs.delete')}
                    </Button>
                  </span>
                </li>
              ))}
            </ul>
          );
        }}
      </Async>
      <Async res={res} label={t('settings.channelTabs.loadingWebhooks')}>
        {(all) => {
          const followers = all.filter((w) => w.type === 2);
          return (
            <section aria-label={t('settings.channelTabs.followedLabel')}>
              <h3 className="ss-sub">{t('settings.channelTabs.followedLabel')}</h3>
              {followers.length === 0 ? (
                <p className="ss-meta">{t('settings.channelTabs.followedNone')}</p>
              ) : (
                <ul className="ss-list">
                  {followers.map((w) => (
                    <li key={w.id} className="ss-row">
                      <span className="ss-grow">{w.name}</span>
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('settings.channelTabs.unfollowNamed', { name: w.name })}
                        onClick={() => setToDelete(w)}
                      >
                        {t('settings.channelTabs.unfollow')}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          );
        }}
      </Async>
      <Modal
        open={creating}
        onClose={() => setCreating(false)}
        size="sm"
        title={t('settings.channelTabs.createWebhook')}
        footer={
          <>
            <Button variant="ghost" onClick={() => setCreating(false)}>
              {t('settings.channelTabs.cancel')}
            </Button>
            <Button loading={action.busy} disabled={!name.trim()} onClick={() => void create()}>
              {t('settings.channelTabs.create')}
            </Button>
          </>
        }
      >
        <TextInput
          label={t('settings.channelTabs.webhookName')}
          value={name}
          maxLength={80}
          autoFocus
          onChange={(e) => setName(e.target.value)}
        />
      </Modal>
      <ConfirmDialog
        open={!!toReset}
        title={t('settings.channelTabs.resetUrlFor', {
          name: toReset?.name ?? t('settings.channelTabs.webhook'),
        })}
        description={t('settings.channelTabs.resetDescription')}
        confirmLabel={t('settings.channelTabs.resetUrl')}
        loading={action.busy}
        onConfirm={() => void resetUrl()}
        onCancel={() => setToReset(null)}
      />
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={t('settings.channelTabs.deleteTitle', {
          name: toDelete?.name ?? t('settings.channelTabs.webhook'),
        })}
        description={t('settings.channelTabs.deleteDescription')}
        confirmLabel={t('settings.channelTabs.deleteWebhook')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

/** Discord's auto-archive choices, in minutes. */
export const ARCHIVE_MINUTES = [
  { value: 60, label: 'settings.channelTabs.archive1h' },
  { value: 1440, label: 'settings.channelTabs.archive24h' },
  { value: 4320, label: 'settings.channelTabs.archive3d' },
  { value: 10080, label: 'settings.channelTabs.archive1w' },
] as const;
export const THREAD_SLOWMODE = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600];
const slow = (s: number) =>
  s === 0
    ? translate('settings.channelTabs.slowOff')
    : s < 60
      ? `${s}s`
      : s < 3600
        ? `${s / 60}m`
        : `${s / 3600}h`;

/**
 * The voice tab of the channel editor (J-09, LNG-CHN-010): the status line under a voice or stage channel's
 * name, over `PUT /channels/{id}/voice-status` (clearing it sends null). The server also ends a status when
 * the last person leaves, and pushes `VOICE_CHANNEL_STATUS_UPDATE`, which the store applies.
 */
export function ChannelVoice({ channel }: { channel: api.ChannelObject }) {
  const t = useT();
  const action = useAction();
  const current = useChannelStatus((s) => s.byChannel[channel.id] ?? '');
  const [text, setText] = useState(current);
  const [edited, setEdited] = useState(false);
  // Follow the live status until the person starts typing.
  useEffect(() => {
    if (!edited) setText(current);
  }, [current, edited]);
  const save = async (next: string | null) => {
    const ok = await action.run(async () => {
      await setChannelStatus(channel.id, next);
      return true;
    }, next ? t('settings.channelTabs.voiceStatusSaved') : t('settings.channelTabs.voiceStatusCleared'));
    if (ok) {
      setEdited(false);
      if (!next) setText('');
    }
  };
  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <TextInput
        label={t('settings.channelTabs.voiceStatusLabel')}
        hint={t('settings.channelTabs.voiceStatusHint')}
        value={text}
        maxLength={MAX_STATUS}
        onChange={(e) => {
          setText(e.target.value);
          setEdited(true);
        }}
      />
      <div className="ss-actions">
        <Button
          loading={action.busy}
          disabled={text.trim() === current}
          onClick={() => void save(text.trim() || null)}
        >
          {t('settings.channelTabs.voiceStatusSave')}
        </Button>
        <Button
          variant="secondary"
          disabled={action.busy || (!current && !text)}
          onClick={() => void save(null)}
        >
          {t('settings.channelTabs.voiceStatusClear')}
        </Button>
      </div>
    </div>
  );
}

/** Thread defaults for a text or announcement channel. Saved as soon as a value changes. */
export function ChannelFeatures({
  channel,
  onChanged,
}: {
  channel: api.ChannelObject;
  onChanged: () => Promise<void>;
}) {
  const t = useT();
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

  if (channel.type === 15 || channel.type === 16)
    return <ForumSettings channel={channel} onChanged={onChanged} />;
  if (!hostsThreads) return <p className="ss-na">{t('settings.channelTabs.noFeatures')}</p>;
  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <h3 className="ss-sub-title">{t('settings.channelTabs.threads')}</h3>
      <div>
        <label className="ss-field-label" htmlFor="ss-thread-archive">
          {t('settings.channelTabs.hideAfter')}
        </label>
        <select
          id="ss-thread-archive"
          className="ss-select"
          value={archive}
          disabled={action.busy}
          onChange={(e) => {
            const v = Number(e.target.value);
            setArchive(v);
            void save({ default_auto_archive_duration: v }, t('settings.channelTabs.saved'));
          }}
        >
          {ARCHIVE_MINUTES.map((o) => (
            <option key={o.value} value={o.value}>
              {t(o.label)}
            </option>
          ))}
        </select>
      </div>
      <div>
        <label className="ss-field-label" htmlFor="ss-thread-slow">
          {t('settings.channelTabs.slowmodeNew')}
        </label>
        <select
          id="ss-thread-slow"
          className="ss-select"
          value={slowmode}
          disabled={action.busy}
          onChange={(e) => {
            const v = Number(e.target.value);
            setSlowmode(v);
            void save({ default_thread_rate_limit_per_user: v }, t('settings.channelTabs.saved'));
          }}
        >
          {THREAD_SLOWMODE.map((s) => (
            <option key={s} value={s}>
              {slow(s)}
            </option>
          ))}
        </select>
      </div>
      <p className="ss-na">{t('settings.channelTabs.featuresNote')}</p>
    </div>
  );
}

const MAX_TAGS = 20;

/** Forum and media settings: tags, default reaction, sort order and layout (LNG-FRM-006). */
export function ForumSettings({
  channel,
  onChanged,
}: {
  channel: api.ChannelObject;
  onChanged: () => Promise<void>;
}) {
  const t = useT();
  const action = useAction();
  const [tags, setTags] = useState<api.ForumTag[]>(channel.available_tags ?? []);
  const [reaction, setReaction] = useState(channel.default_reaction_emoji?.emoji_name ?? '');
  const [sort, setSort] = useState(String(channel.default_sort_order ?? 0));
  const [layout, setLayout] = useState(String(channel.default_forum_layout ?? 0));
  const patchTag = (i: number, patch: Partial<api.ForumTag>) =>
    setTags((list) => list.map((tag, k) => (k === i ? { ...tag, ...patch } : tag)));
  const valid = tags.every((tag) => tag.name.trim().length > 0);

  const save = async () => {
    const ok = await action.run(async () => {
      await api.modifyChannel(channel.id, {
        available_tags: tags.map((tag) => ({ ...tag, name: tag.name.trim() })),
        default_reaction_emoji: reaction.trim() ? { emoji_name: reaction.trim() } : null,
        default_sort_order: Number(sort),
        default_forum_layout: Number(layout),
      });
      return true;
    }, t('settings.channelTabs.saved'));
    if (ok) await onChanged();
  };

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <h3 className="ss-sub-title">{t('settings.forumSettings.tags')}</h3>
      <ul className="ss-list">
        {tags.map((tag, i) => (
          <li key={tag.id ?? `new-${i}`} className="ss-row">
            <TextInput
              label={t('settings.forumSettings.tagName', { n: i + 1 })}
              value={tag.name}
              maxLength={20}
              onChange={(e) => patchTag(i, { name: e.target.value })}
            />
            <label>
              <input
                type="checkbox"
                checked={!!tag.moderated}
                onChange={(e) => patchTag(i, { moderated: e.target.checked })}
              />{' '}
              {t('settings.forumSettings.moderated')}
            </label>
            <Button
              variant="ghost"
              onClick={() => setTags((list) => list.filter((_, k) => k !== i))}
            >
              {t('settings.forumSettings.removeTag', { n: i + 1 })}
            </Button>
          </li>
        ))}
      </ul>
      <Button
        variant="secondary"
        disabled={tags.length >= MAX_TAGS}
        onClick={() => setTags((list) => [...list, { name: '' }])}
      >
        {t('settings.forumSettings.addTag')}
      </Button>
      <div>
        <label className="ss-field-label" htmlFor="ss-forum-reaction">
          {t('settings.forumSettings.reaction')}
        </label>
        <input
          id="ss-forum-reaction"
          className="ss-select"
          value={reaction}
          maxLength={8}
          onChange={(e) => setReaction(e.target.value)}
        />
      </div>
      <div>
        <label className="ss-field-label" htmlFor="ss-forum-sort">
          {t('settings.forumSettings.sort')}
        </label>
        <select
          id="ss-forum-sort"
          className="ss-select"
          value={sort}
          onChange={(e) => setSort(e.target.value)}
        >
          <option value="0">{t('settings.forumSettings.sortActivity')}</option>
          <option value="1">{t('settings.forumSettings.sortCreated')}</option>
        </select>
      </div>
      <div>
        <label className="ss-field-label" htmlFor="ss-forum-layout">
          {t('settings.forumSettings.layout')}
        </label>
        <select
          id="ss-forum-layout"
          className="ss-select"
          value={layout}
          onChange={(e) => setLayout(e.target.value)}
        >
          <option value="0">{t('settings.forumSettings.layoutDefault')}</option>
          <option value="1">{t('settings.forumSettings.layoutList')}</option>
          <option value="2">{t('settings.forumSettings.layoutGallery')}</option>
        </select>
      </div>
      <p className="ss-na">{t('settings.forumSettings.guidelinesNote')}</p>
      <div className="ss-actions">
        <Button loading={action.busy} disabled={!valid} onClick={() => void save()}>
          {t('settings.forumSettings.save')}
        </Button>
      </div>
    </div>
  );
}
