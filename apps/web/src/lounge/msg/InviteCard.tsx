import { Button } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
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
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [joining, setJoining] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const navigate = useNavigate();
  const joined = useStore((s) =>
    state.kind === 'ready' ? s.guilds[state.invite.guild.id] !== undefined : false,
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
          <div className="lng-embed-provider">Invite</div>
          <div className="lng-muted">{state.message}</div>
        </div>
      </div>
    );
  }
  const { invite } = state;
  const open = () =>
    void navigate({
      to: '/channels/$guildId/$channelId',
      params: { guildId: invite.guild.id, channelId: invite.channel.id },
    });
  return (
    <section className="lng-embed lng-invite" aria-label={`Invite to ${invite.guild.name}`}>
      <div className="lng-embed-main">
        <div className="lng-embed-provider">You&apos;ve been invited to join a server</div>
        <div className="lng-embed-title">{invite.guild.name}</div>
        <div className="lng-muted lng-invite-counts">
          {invite.approximate_presence_count !== undefined &&
            `${invite.approximate_presence_count.toLocaleString()} Online`}
          {invite.approximate_presence_count !== undefined &&
            invite.approximate_member_count !== undefined &&
            ' • '}
          {invite.approximate_member_count !== undefined &&
            `${invite.approximate_member_count.toLocaleString()} Members`}
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
        {joined ? 'Joined' : 'Join'}
      </Button>
    </section>
  );
}
