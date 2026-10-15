import { Button } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { formatNumber, useT } from '../../i18n';
import { rest, useStore } from '../../store';
import { inviteErrorMessage } from '../inviteFlow';

/** An invite link in this site's own address space: `https://this-host/invite/CODE`. */
export function findInviteCode(content: string, origin: string): string | null {
  const re = /https?:\/\/[^\s<>]+/g;
  for (const url of content.match(re) ?? []) {
    let u: URL;
    try {
      u = new URL(url);
    } catch {
      continue;
    }
    if (u.origin !== origin) continue;
    const m = /^\/(?:invite|join)\/([A-Za-z0-9-]{2,32})\/?$/.exec(u.pathname);
    if (m) return m[1]!;
  }
  return null;
}

type State =
  { kind: 'loading' } | { kind: 'ready'; invite: rest.Invite } | { kind: 'error'; message: string };

/** Server invite embed (LNG-MSGUI-019): name, member counts and a Join button. */
export function InviteCard({ code }: { code: string }) {
  const t = useT();
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [joining, setJoining] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const navigate = useNavigate();
  const joined = useStore((s) =>
    state.kind === 'ready'
      ? state.invite.type !== 1 && state.invite.guild
        ? s.guilds[state.invite.guild.id] !== undefined
        : s.channels[state.invite.channel.id] !== undefined
      : false,
  );

  useEffect(() => {
    let live = true;
    rest
      .getInvite(code)
      .then((invite) => live && setState({ kind: 'ready', invite }))
      .catch((e: unknown) => live && setState({ kind: 'error', message: inviteErrorMessage(e) }));
    return () => {
      live = false;
    };
  }, [code]);

  if (state.kind === 'loading') return null;
  if (state.kind === 'error') {
    return (
      <div className="lng-embed lng-invite" role="note">
        <div className="lng-embed-main">
          <div className="lng-embed-provider">{t('lounge.inviteCard.invite')}</div>
          <div className="lng-muted">{state.message}</div>
        </div>
      </div>
    );
  }
  const { invite } = state;
  const name =
    invite.type !== 1
      ? (invite.guild?.name ?? t('lounge.inviteCard.invite'))
      : (invite.channel.name ?? t('lounge.inviteCard.group'));
  const open = () =>
    void navigate({
      to: '/channels/$guildId/$channelId',
      params: {
        guildId: invite.type !== 1 ? (invite.guild?.id ?? '@me') : '@me',
        channelId: invite.channel.id,
      },
    });
  return (
    <section
      className="lng-embed lng-invite"
      aria-label={t('lounge.inviteCard.inviteTo', { name })}
    >
      <div className="lng-embed-main">
        <div className="lng-embed-provider">{t('lounge.inviteCard.invitedToJoin')}</div>
        <div className="lng-embed-title">{name}</div>
        <div className="lng-muted lng-invite-counts">
          {invite.approximate_presence_count !== undefined &&
            t('lounge.inviteCard.online', {
              count: formatNumber(invite.approximate_presence_count),
            })}
          {invite.approximate_presence_count !== undefined &&
            invite.approximate_member_count !== undefined &&
            ' • '}
          {invite.approximate_member_count !== undefined &&
            t('lounge.inviteCard.members', {
              count: formatNumber(invite.approximate_member_count),
            })}
        </div>
        {err && (
          <p className="lng-failed" role="alert">
            {err}
          </p>
        )}
      </div>
      <Button
        variant={joined ? 'secondary' : 'primary'}
        loading={joining}
        onClick={() => {
          if (joined) return open();
          setJoining(true);
          useStore
            .getState()
            .acceptInvite(code)
            .then(open)
            .catch((e: unknown) => setErr(inviteErrorMessage(e)))
            .finally(() => setJoining(false));
        }}
      >
        {joined ? t('lounge.inviteCard.joined') : t('lounge.inviteCard.join')}
      </Button>
    </section>
  );
}
