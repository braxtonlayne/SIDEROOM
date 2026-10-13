import { Button, ThemeProvider } from '@sideroom/ui';
import { Link, useNavigate, useParams } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { getMe, getServerInvite, type ServerInvitePreview } from '../../api/entrance';
import { inviteErrorMessage } from '../../lounge/inviteFlow';
import { useStore } from '../../store';
import '../../components/entrance.css';

type State =
  | { kind: 'loading' }
  | { kind: 'invalid' }
  | { kind: 'ready'; invite: ServerInvitePreview; signedIn: boolean };

const initial = (name: string) => name.slice(0, 1).toUpperCase();

/**
 * Server invite page (D-021). Logged out: server name, icon and counts, plus "Log in to accept" (no register
 * button, no link to the landing page). Logged in: the same card with an "Accept invite" button that joins
 * the server and opens it in the Lounge.
 */
export function ServerInvite() {
  const { code } = useParams({ strict: false }) as { code: string };
  const navigate = useNavigate();
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [accepting, setAccepting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    const signedIn = getMe().then(
      () => true,
      () => false,
    );
    Promise.all([getServerInvite(code), signedIn])
      .then(([invite, isIn]) => live && setState({ kind: 'ready', invite, signedIn: isIn }))
      .catch((e: unknown) => {
        if (!live) return;
        // Previews need a session; a 401 still means "log in to accept", just without details.
        if (e instanceof ApiError && e.status === 401)
          setState({
            kind: 'ready',
            signedIn: false,
            invite: {
              code,
              guild: { id: '', name: 'a server', icon: null },
              approximate_member_count: 0,
              approximate_presence_count: 0,
            },
          });
        else setState({ kind: 'invalid' });
      });
    return () => {
      live = false;
    };
  }, [code]);

  const accept = async () => {
    setAccepting(true);
    setError(null);
    try {
      const joined = await useStore.getState().acceptInvite(code);
      await navigate({
        to: '/channels/$guildId/$channelId',
        params: { guildId: joined.guild.id, channelId: joined.channel.id },
      });
    } catch (e) {
      setError(inviteErrorMessage(e));
      setAccepting(false);
    }
  };

  const counts = (inv: ServerInvitePreview) =>
    inv.approximate_member_count || inv.approximate_presence_count ? (
      <p>
        {(inv.approximate_presence_count ?? 0).toLocaleString()} online ·{' '}
        {(inv.approximate_member_count ?? 0).toLocaleString()} members
      </p>
    ) : null;

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
                initial(state.invite.guild.name)
              )}
            </div>
            <p>You’ve been invited to join</p>
            <h1>{state.invite.guild.name}</h1>
            {counts(state.invite)}
            {state.signedIn ? (
              <>
                <Button
                  className="ent-invite-cta"
                  loading={accepting}
                  onClick={() => void accept()}
                >
                  Accept invite
                </Button>
                {error && <p role="alert">{error}</p>}
              </>
            ) : (
              <Link to="/login" search={{ next: `/invite/${code}` }} className="ent-invite-cta">
                Log in to accept
              </Link>
            )}
          </>
        )}
      </main>
    </ThemeProvider>
  );
}
