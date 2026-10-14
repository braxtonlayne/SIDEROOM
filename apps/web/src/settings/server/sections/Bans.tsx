import { Avatar, Button, ConfirmDialog, EmptyState, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

export function BansSection({ guildId }: SectionProps) {
  const t = useT();
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
      t('settings.bans.unbanned', { name: api.userName(target.user) }),
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
        title={t('settings.bans.title')}
        description={t('settings.bans.description')}
      />
      <div className="ss-toolbar">
        <div className="ss-search">
          <TextInput
            label={t('settings.bans.searchLabel')}
            type="search"
            value={query}
            placeholder={t('settings.bans.searchPlaceholder')}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>
      <ActionStatus error={action.error} done={action.done} />
      <Async res={res} label={t('settings.bans.loading')}>
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
            return (
              <EmptyState
                title={t('settings.bans.emptyTitle')}
                description={t('settings.bans.emptyDescription')}
              />
            );
          return (
            <>
              <ul className="ss-list" aria-label={t('settings.bans.listLabel')}>
                {shown.map((b) => (
                  <li key={b.user.id} className="ss-row">
                    <Avatar name={api.userName(b.user)} size={32} />
                    <div className="ss-grow">
                      <div className="ss-name">{api.userName(b.user)}</div>
                      <div className="ss-meta">
                        {b.reason
                          ? t('settings.bans.reason', { reason: b.reason })
                          : t('settings.bans.noReason')}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('settings.bans.revokeFor', { name: api.userName(b.user) })}
                      onClick={() => setTarget(b)}
                    >
                      {t('settings.bans.revoke')}
                    </Button>
                  </li>
                ))}
              </ul>
              {shown.length === 0 && <p className="ss-na">{t('settings.bans.noMatch')}</p>}
            </>
          );
        }}
      </Async>
      <ConfirmDialog
        open={!!target}
        title={t('settings.bans.revokeFor', { name: target ? api.userName(target.user) : '' })}
        description={t('settings.bans.confirmDescription')}
        confirmLabel={t('settings.bans.revoke')}
        loading={action.busy}
        onConfirm={() => void unban()}
        onCancel={() => setTarget(null)}
      />
    </>
  );
}
