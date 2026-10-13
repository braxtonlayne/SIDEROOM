import { Avatar, Button, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import type { rest } from '../../store';
import { sendServerInvite } from './userMenu';
import { friendsOf, useSocial, wireName } from './social';

type Sent = 'sending' | 'sent' | 'error';

/**
 * Friend list with an Invite button each (LNG-INV-007). Every friend gets their own invite, delivered as a
 * direct message. The invite stays on the platform, so it uses the server defaults unless the dialog's
 * pickers were edited (`options`, D-182).
 */
export function InviteFriends({
  channelId,
  options,
}: {
  channelId: string;
  options?: rest.CreateInviteOptions | undefined;
}) {
  const rows = useSocial((s) => s.relationships);
  const load = useSocial((s) => s.loadRelationships);
  const [query, setQuery] = useState('');
  const [sent, setSent] = useState<Record<string, Sent>>({});

  useEffect(() => {
    void load();
  }, [load]);

  const friends = friendsOf(rows);
  if (friends.length === 0) return null;
  const q = query.trim().toLowerCase();
  const shown = friends.filter(
    (r) =>
      !q || wireName(r.user).toLowerCase().includes(q) || r.user.username.toLowerCase().includes(q),
  );

  const invite = async (id: string) => {
    if (!channelId) return;
    setSent((s) => ({ ...s, [id]: 'sending' }));
    try {
      await sendServerInvite(channelId, id, options);
      setSent((s) => ({ ...s, [id]: 'sent' }));
    } catch {
      setSent((s) => ({ ...s, [id]: 'error' }));
    }
  };

  return (
    <section aria-label="Invite friends">
      <h3 className="lng-form-h">Invite friends</h3>
      <TextInput
        label="Search friends"
        value={query}
        placeholder="Search for friends"
        onChange={(e) => setQuery(e.target.value)}
      />
      <ul className="lng-pick-list" aria-label="Friends">
        {shown.map((r) => {
          const state = sent[r.id];
          return (
            <li key={r.id} className="lng-pick-row">
              <Avatar name={wireName(r.user)} size={32} />
              <span className="lng-friend-text">
                <span className="lng-friend-name">{wireName(r.user)}</span>
                {state === 'error' && (
                  <span role="alert" className="lng-form-error">
                    Could not send the invite.
                  </span>
                )}
              </span>
              <Button
                size="sm"
                variant="secondary"
                loading={state === 'sending'}
                disabled={state === 'sent'}
                aria-label={`Invite ${wireName(r.user)}`}
                onClick={() => void invite(r.id)}
              >
                {state === 'sent' ? 'Sent' : state === 'error' ? 'Retry' : 'Invite'}
              </Button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
