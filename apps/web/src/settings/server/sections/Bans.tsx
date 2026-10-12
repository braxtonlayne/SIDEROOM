import { Avatar, Button, ConfirmDialog, EmptyState, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

export function BansSection({ guildId }: SectionProps) {
  const res = useResource(`bans:${guildId}`, () => api.listBans(guildId));
  useGuildEvents(guildId, ['GUILD_BAN_ADD', 'GUILD_BAN_REMOVE'], () => void res.reload());
  const [query, setQuery] = useState('');
  const [target, setTarget] = useState<api.BanObject | null>(null);
  const action = useAction();

  const unban = async () => {
    if (!target) return;
    const ok = await action.run(
      async () => {
        await api.unban(guildId, target.user.id);
        return true;
      },
      `${api.userName(target.user)} was unbanned.`,
    );
    if (ok) {
      res.set((cur) => (cur ?? []).filter((b) => b.user.id !== target.user.id));
      setTarget(null);
      await res.reload();
    }
  };

  return (
    <>
      <SectionHeader
        title="Bans"
        description="Banned people cannot rejoin the server until you revoke the ban."
      />
      <div className="ss-toolbar">
        <div className="ss-search">
          <TextInput
            label="Search bans"
            type="search"
            value={query}
            placeholder="Search by name, user ID or reason"
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>
      <ActionStatus error={action.error} done={action.done} />
      <Async res={res} label="Loading bans">
        {(bans) => {
          const q = query.trim().toLowerCase();
          const shown = q
            ? bans.filter(
                (b) =>
                  api.userName(b.user).toLowerCase().includes(q) ||
                  (b.user.username ?? '').toLowerCase().includes(q) ||
                  b.user.id.includes(q) ||
                  (b.reason ?? '').toLowerCase().includes(q),
              )
            : bans;
          if (bans.length === 0)
            return <EmptyState title="No bans" description="Nobody is banned from this server." />;
          return (
            <>
              <ul className="ss-list" aria-label="Banned users">
                {shown.map((b) => (
                  <li key={b.user.id} className="ss-row">
                    <Avatar name={api.userName(b.user)} size={32} />
                    <div className="ss-grow">
                      <div className="ss-name">{api.userName(b.user)}</div>
                      <div className="ss-meta">
                        {b.reason ? `Reason: ${b.reason}` : 'No reason given'}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={`Revoke ban for ${api.userName(b.user)}`}
                      onClick={() => setTarget(b)}
                    >
                      Revoke ban
                    </Button>
                  </li>
                ))}
              </ul>
              {shown.length === 0 && <p className="ss-na">No bans match your search.</p>}
            </>
          );
        }}
      </Async>
      <ConfirmDialog
        open={!!target}
        title={`Revoke ban for ${target ? api.userName(target.user) : ''}`}
        description="They will be able to join again with a new invite."
        confirmLabel="Revoke ban"
        loading={action.busy}
        onConfirm={() => void unban()}
        onCancel={() => setTarget(null)}
      />
    </>
  );
}
