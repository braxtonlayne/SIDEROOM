import { Button, Modal } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ApiError } from '../api/client';
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
      .catch(() => live && setListError('Could not load existing invites.'));
    return () => {
      live = false;
    };
  }, [canList, manageGuild, guildId, listScope]);

  // Client-side hiding is a convenience; the server enforces CREATE_INSTANT_INVITE.
  if (!guild?.permissions.createInvite) return null;

  const explicit: rest.CreateInviteOptions | undefined = edited
    ? { max_age: maxAge, max_uses: maxUses }
    : undefined;

  const generate = async () => {
    if (!channelId) return setError('Pick a channel to invite people to.');
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
          ? 'You cannot create invites for this channel.'
          : err instanceof ApiError
            ? err.message
            : 'Could not create the invite. Try again.',
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
      setError('Copying is blocked by the browser. Select the link and copy it by hand.');
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
      setError(err instanceof ApiError ? err.message : 'Could not revoke the invite.');
    }
  };

  return (
    <Modal open onClose={onClose} title={`Invite people to ${guild.name}`} size="md">
      <div className="lng-form">
        <label className="lng-field">
          <span>Invite to channel</span>
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
            <span>Expire after</span>
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
            <span>Max number of uses</span>
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
            Copy link
          </Button>
        </div>
        {created && (
          <div className="lng-invite-link">
            <input
              readOnly
              aria-label="Invite link"
              value={inviteUrl(created.code)}
              onFocus={(e) => e.currentTarget.select()}
            />
            <Button onClick={() => void copy(created.code)}>
              {copied === created.code ? 'Copied!' : 'Copy'}
            </Button>
          </div>
        )}
        {created && (
          <p className="lng-muted" data-testid="invite-note">
            {created.max_uses === 1
              ? 'This link can be used once'
              : created.max_uses
                ? `This link can be used ${created.max_uses} times`
                : 'This link can be used any number of times'}
            {' · '}
            {expiryLabel(created.expires_at)}
          </p>
        )}
        {canList && (
          <section aria-label="Active invites">
            <h3 className="lng-form-h">Active invites</h3>
            {listError && <p role="alert">{listError}</p>}
            {!listError && invites.length === 0 && (
              <p className="lng-muted">No active invites yet.</p>
            )}
            <ul className="lng-invite-list">
              {invites.map((inv) => (
                <li key={inv.code}>
                  <div>
                    <code>{inv.code}</code>
                    <small>
                      {inv.uses}
                      {inv.max_uses ? ` / ${inv.max_uses}` : ''} uses ·{' '}
                      {expiryLabel(inv.expires_at)}
                      {inv.channel.name ? ` · #${inv.channel.name}` : ''}
                    </small>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`Copy link for ${inv.code}`}
                    onClick={() => void copy(inv.code)}
                  >
                    {copied === inv.code ? 'Copied!' : 'Copy link'}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Revoke invite ${inv.code}`}
                    onClick={() => void revoke(inv.code)}
                  >
                    Revoke
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
