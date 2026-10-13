import { Button, Modal } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ApiError } from '../api/client';
import { rest } from '../store';
import { isTextLike } from './channels';
import { useChannels, useGuild } from './data';
import { EXPIRY_OPTIONS, expiryLabel, inviteUrl, USES_OPTIONS } from './inviteFlow';
import { useLoungeRoute } from './nav';

/**
 * "Invite people": create an invite (max_age / max_uses), copy the link, list and revoke existing ones.
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
  const [maxAge, setMaxAge] = useState(86400);
  const [maxUses, setMaxUses] = useState(0);
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

  const generate = async () => {
    if (!channelId) return setError('Pick a channel to invite people to.');
    setBusy(true);
    setError(null);
    try {
      const inv = await rest.createInvite(channelId, { max_age: maxAge, max_uses: maxUses });
      setCreated(inv);
      setInvites((l) => [inv, ...l.filter((x) => x.code !== inv.code)]);
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
        <div className="lng-field-row">
          <label className="lng-field">
            <span>Expire after</span>
            <select value={maxAge} onChange={(e) => setMaxAge(Number(e.target.value))}>
              {EXPIRY_OPTIONS.map((o) => (
                <option key={o.seconds} value={o.seconds}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          <label className="lng-field">
            <span>Max number of uses</span>
            <select value={maxUses} onChange={(e) => setMaxUses(Number(e.target.value))}>
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
            Generate invite link
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
