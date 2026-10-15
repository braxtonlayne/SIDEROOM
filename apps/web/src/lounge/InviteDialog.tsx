import { Button, Modal } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ApiError } from '../api/client';
import { useStreamer } from '../settings/user/prefs';
import { useT } from '../i18n';
import { rest } from '../store';
import { isTextLike } from './channels';
import { useChannels, useGuild } from './data';
import {
  EXPIRY_OPTIONS,
  EXTERNAL_MAX_AGE,
  EXTERNAL_MAX_USES,
  expiryLabel,
  inviteUrl,
  USES_OPTIONS,
} from './inviteFlow';
import { useLoungeRoute } from './nav';
import { InviteFriends } from './shell/InviteFriends';

/**
 * "Invite people": create an invite (max_age / max_uses), copy the link, list and revoke existing ones.
 * "Copy link" makes an external invite (single use, 7 days; D-022, D-182) and "Invite friends" DMs use the
 * server defaults (7 days, unlimited). The pickers override either once the person touches them.
 * Listing needs MANAGE_GUILD (guild-wide) or MANAGE_CHANNELS (per channel); others only create.
 */
export function InviteDialog({
  guildId,
  channelId: initialChannel,
  onClose,
}: {
  guildId: string;
  channelId?: string | undefined;
  onClose: () => void;
}) {
  const t = useT();
  const hideLinks = useStreamer().invites;
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const route = useLoungeRoute();
  const textChannels = useMemo(() => channels.filter(isTextLike), [channels]);
  const [channelId, setChannelId] = useState(
    initialChannel ??
      (route.guildId === guildId ? route.channelId : undefined) ??
      textChannels[0]?.id ??
      '',
  );
  // The pickers start on the external defaults; until touched, requests send no explicit limits.
  const [maxAge, setMaxAge] = useState(EXTERNAL_MAX_AGE);
  const [maxUses, setMaxUses] = useState(EXTERNAL_MAX_USES);
  const [edited, setEdited] = useState(false);
  const [created, setCreated] = useState<rest.Invite | null>(null);
  const [invites, setInvites] = useState<rest.Invite[]>([]);
  const [listError, setListError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(copyTimer.current), []);

  // The default channel may only arrive once the guild's channels have loaded.
  const firstChannel = textChannels[0]?.id ?? '';
  useEffect(() => {
    if (!channelId && firstChannel) setChannelId(firstChannel);
  }, [channelId, firstChannel]);

  const manageGuild = !!guild?.permissions.manageGuild;
  const canList = manageGuild || !!guild?.permissions.manageChannels;
  // Guild managers list the whole guild once; channel managers list the selected channel.
  const listScope = manageGuild ? guildId : channelId;
  useEffect(() => {
    if (!canList) return;
    if (!listScope) return;
    let live = true;
    (manageGuild ? rest.listGuildInvites(guildId) : rest.listChannelInvites(listScope))
      .then((list) => live && (setInvites(list), setListError(null)))
      .catch(() => live && setListError(t('lounge.inviteDialog.listFailed')));
    return () => {
      live = false;
    };
  }, [canList, manageGuild, guildId, listScope, t]);

  // Client-side hiding is a convenience; the server enforces CREATE_INSTANT_INVITE.
  if (!guild?.permissions.createInvite) return null;

  const explicit: rest.CreateInviteOptions | undefined = edited
    ? { max_age: maxAge, max_uses: maxUses }
    : undefined;

  const generate = async () => {
    if (!channelId) return setError(t('lounge.inviteDialog.pickChannel'));
    setBusy(true);
    setError(null);
    try {
      const inv = await rest.createInvite(channelId, { external: true, ...explicit });
      setCreated(inv);
      setInvites((l) => [inv, ...l.filter((x) => x.code !== inv.code)]);
      await copy(inv.code);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? t('lounge.inviteDialog.forbidden')
          : err instanceof ApiError
            ? err.message
            : t('lounge.inviteDialog.createFailed'),
      );
    } finally {
      setBusy(false);
    }
  };

  const copy = async (code: string) => {
    try {
      await navigator.clipboard.writeText(inviteUrl(code));
      setCopied(code);
    } catch {
      setError(t('lounge.inviteDialog.copyBlocked'));
      return;
    }
    clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopied(null), 2000);
  };

  const revoke = async (code: string) => {
    try {
      await rest.revokeInvite(code);
      setInvites((l) => l.filter((x) => x.code !== code));
      setCreated((c) => (c?.code === code ? null : c));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : t('lounge.inviteDialog.revokeFailed'));
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={t('lounge.inviteDialog.title', { name: guild.name })}
      size="md"
    >
      <div className="lng-form">
        <label className="lng-field">
          <span>{t('lounge.inviteDialog.inviteToChannel')}</span>
          <select value={channelId} onChange={(e) => setChannelId(e.target.value)}>
            {textChannels.map((c) => (
              <option key={c.id} value={c.id}>
                #{c.name}
              </option>
            ))}
          </select>
        </label>
        {channelId && <InviteFriends channelId={channelId} options={explicit} />}
        <div className="lng-field-row">
          <label className="lng-field">
            <span>{t('lounge.inviteDialog.expireAfter')}</span>
            <select
              value={maxAge}
              onChange={(e) => {
                setMaxAge(Number(e.target.value));
                setEdited(true);
              }}
            >
              {EXPIRY_OPTIONS.map((o) => (
                <option key={o.seconds} value={o.seconds}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          <label className="lng-field">
            <span>{t('lounge.inviteDialog.maxUses')}</span>
            <select
              value={maxUses}
              onChange={(e) => {
                setMaxUses(Number(e.target.value));
                setEdited(true);
              }}
            >
              {USES_OPTIONS.map((o) => (
                <option key={o.uses} value={o.uses}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        {error && (
          <p role="alert" className="lng-form-error">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button loading={busy} onClick={() => void generate()}>
            {t('lounge.inviteDialog.copyLink')}
          </Button>
        </div>
        {created && (
          <div className="lng-invite-link">
            <input
              readOnly
              type={hideLinks ? 'password' : 'text'}
              aria-label={t('lounge.inviteDialog.inviteLink')}
              value={inviteUrl(created.code)}
              onFocus={(e) => e.currentTarget.select()}
            />
            <Button onClick={() => void copy(created.code)}>
              {copied === created.code
                ? t('lounge.inviteDialog.copied')
                : t('lounge.inviteDialog.copy')}
            </Button>
          </div>
        )}
        {created && (
          <p className="lng-muted" data-testid="invite-note">
            {created.max_uses === 1
              ? t('lounge.inviteDialog.usedOnce')
              : created.max_uses
                ? t('lounge.inviteDialog.usedTimes', { count: created.max_uses })
                : t('lounge.inviteDialog.usedAny')}
            {' · '}
            {expiryLabel(created.expires_at)}
          </p>
        )}
        {canList && (
          <section aria-label={t('lounge.inviteDialog.activeInvites')}>
            <h3 className="lng-form-h">{t('lounge.inviteDialog.activeInvites')}</h3>
            {listError && <p role="alert">{listError}</p>}
            {!listError && invites.length === 0 && (
              <p className="lng-muted">{t('lounge.inviteDialog.noInvites')}</p>
            )}
            <ul className="lng-invite-list">
              {invites.map((inv) => (
                <li key={inv.code}>
                  <div>
                    <code>{hideLinks ? '••••••' : inv.code}</code>
                    <small>
                      {inv.max_uses
                        ? t('lounge.inviteDialog.usesOf', { uses: inv.uses, max: inv.max_uses })
                        : t('lounge.inviteDialog.usesOnly', { uses: inv.uses })}{' '}
                      {expiryLabel(inv.expires_at)}
                      {inv.channel.name ? ` · #${inv.channel.name}` : ''}
                    </small>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={t('lounge.inviteDialog.copyLinkFor', { code: inv.code })}
                    onClick={() => void copy(inv.code)}
                  >
                    {copied === inv.code
                      ? t('lounge.inviteDialog.copied')
                      : t('lounge.inviteDialog.copyLink')}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('lounge.inviteDialog.revokeInvite', { code: inv.code })}
                    onClick={() => void revoke(inv.code)}
                  >
                    {t('lounge.inviteDialog.revoke')}
                  </Button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </Modal>
  );
}
