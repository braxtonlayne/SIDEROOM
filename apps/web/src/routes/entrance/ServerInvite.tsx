import { ThemeProvider } from '@sideroom/ui';
import { Link, useParams } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { getServerInvite, type ServerInvitePreview } from '../../api/entrance';
import '../../components/entrance.css';

type State =
  { kind: 'loading' } | { kind: 'invalid' } | { kind: 'ready'; invite: ServerInvitePreview };

/**
 * Logged-out server invite (D-021): server name, icon and counts, plus "Log in to accept".
 * Deliberately no register button and no link to the landing page.
 */
export function ServerInvite() {
  const { code } = useParams({ strict: false }) as { code: string };
  const [state, setState] = useState<State>({ kind: 'loading' });

  useEffect(() => {
    let live = true;
    getServerInvite(code)
      .then((invite) => live && setState({ kind: 'ready', invite }))
      .catch(() => live && setState({ kind: 'invalid' }));
    return () => {
      live = false;
    };
  }, [code]);

  return (
    <ThemeProvider section="lounge" className="ent-invite-page">
      <main className="ent-invite-card" aria-busy={state.kind === 'loading'}>
        {state.kind === 'loading' && <p role="status">Loading invite…</p>}
        {state.kind === 'invalid' && <p role="alert">This invite is invalid or has expired.</p>}
        {state.kind === 'ready' && (
          <>
            <div className="ent-invite-icon" aria-hidden="true">
              {state.invite.guild.icon ? (
                <img
                  src={`/cdn/icons/${state.invite.guild.id}/${state.invite.guild.icon}.png`}
                  alt=""
                  width={72}
                  height={72}
                />
              ) : (
                state.invite.guild.name.slice(0, 1).toUpperCase()
              )}
            </div>
            <p>You’ve been invited to join</p>
            <h1>{state.invite.guild.name}</h1>
            <p>
              {state.invite.approximate_presence_count.toLocaleString()} online ·{' '}
              {state.invite.approximate_member_count.toLocaleString()} members
            </p>
            <Link to="/login" search={{ next: `/invite/${code}` }} className="ent-invite-cta">
              Log in to accept
            </Link>
          </>
        )}
      </main>
    </ThemeProvider>
  );
}
