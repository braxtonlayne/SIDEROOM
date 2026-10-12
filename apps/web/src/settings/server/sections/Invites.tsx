import { Button, EmptyState, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { InviteDialog } from '../../../lounge/InviteDialog';
import { expiryLabel, inviteUrl } from '../../../lounge/inviteFlow';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, formatDate, useAction } from '../ui';

export function InvitesSection({ guildId }: SectionProps) {
  const res = useResource(`invites:${guildId}`, () => api.listGuildInvites(guildId));
  const community = useResource(`community:${guildId}`, () => api.getCommunity(guildId));
  useGuildEvents(guildId, ['INVITE_CREATE', 'INVITE_DELETE'], () => void res.reload());
  const action = useAction();
  const pause = useAction();
  const [creating, setCreating] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  const revoke = async (code: string) => {
    const ok = await action.run(async () => {
      await api.revokeInvite(code);
      return true;
    });
    if (ok) res.set((cur) => (cur ?? []).filter((i) => i.code !== code));
  };
  const copy = async (code: string) => {
    try {
      await navigator.clipboard.writeText(inviteUrl(code));
      setCopied(code);
      setTimeout(() => setCopied((c) => (c === code ? null : c)), 2000);
    } catch {
      action.setError('Copying is blocked by the browser. Select the link and copy it by hand.');
    }
  };
  const togglePause = async (paused: boolean) => {
    const next = await pause.run(() => api.modifyCommunity(guildId, { invites_paused: paused }));
    if (next) community.set(next);
  };

  return (
    <>
      <SectionHeader
        title="Invites"
        description="Everyone who joined through one of these links. Revoke a link to stop new joins."
        actions={<Button onClick={() => setCreating(true)}>Create invite</Button>}
      />
      <div className="ss-card" style={{ marginBottom: 16 }}>
        {community.data ? (
          <Switch
            label="Pause invites"
            description="While paused nobody can join with an invite link. Existing members are not affected."
            checked={community.data.invites_paused}
            disabled={pause.busy}
            onChange={(e) => void togglePause(e.target.checked)}
          />
        ) : community.error ? (
          <p className="ss-na">Could not load the pause setting: {community.error}</p>
        ) : (
          <p className="ss-na">Loading invite settings...</p>
        )}
        <ActionStatus error={pause.error} />
      </div>
      <ActionStatus error={action.error} />
      <Async res={res} label="Loading invites">
        {(invites) =>
          invites.length === 0 ? (
            <EmptyState
              title="No active invites"
              description="Create an invite to bring people in."
            />
          ) : (
            <table className="ss-table">
              <thead>
                <tr>
                  <th scope="col">Inviter</th>
                  <th scope="col">Code</th>
                  <th scope="col">Uses</th>
                  <th scope="col">Expires</th>
                  <th scope="col">Channel</th>
                  <th scope="col">
                    <span className="ss-sr">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {invites.map((i) => (
                  <tr key={i.code}>
                    <td>{api.userName(i.inviter as api.UserRef | undefined, 'Unknown')}</td>
                    <td>
                      <code>{i.code}</code>
                      <div className="ss-meta">{formatDate(i.created_at)}</div>
                    </td>
                    <td>
                      {i.uses}
                      {i.max_uses ? ` / ${i.max_uses}` : ''}
                    </td>
                    <td>{expiryLabel(i.expires_at)}</td>
                    <td>{i.channel.name ? `#${i.channel.name}` : ''}</td>
                    <td>
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
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )
        }
      </Async>
      {creating && (
        <InviteDialog
          guildId={guildId}
          onClose={() => {
            setCreating(false);
            void res.reload();
          }}
        />
      )}
    </>
  );
}
