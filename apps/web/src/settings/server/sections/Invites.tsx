import { Button, EmptyState, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { InviteDialog } from '../../../lounge/InviteDialog';
import { expiryLabel, inviteUrl } from '../../../lounge/inviteFlow';
import * as api from '../api';
import { useGuildEvents, useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, formatDate, useAction } from '../ui';

export function InvitesSection({ guildId }: SectionProps) {
  const t = useT();
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
      action.setError(t('settings.invites.copyBlocked'));
    }
  };
  const togglePause = async (paused: boolean) => {
    const next = await pause.run(() => api.modifyCommunity(guildId, { invites_paused: paused }));
    if (next) community.set(next);
  };

  return (
    <>
      <SectionHeader
        title={t('settings.invites.title')}
        description={t('settings.invites.description')}
        actions={<Button onClick={() => setCreating(true)}>{t('settings.invites.create')}</Button>}
      />
      <div className="ss-card" style={{ marginBottom: 16 }}>
        {community.data ? (
          <Switch
            label={t('settings.invites.pauseLabel')}
            description={t('settings.invites.pauseDescription')}
            checked={community.data.invites_paused}
            disabled={pause.busy}
            onChange={(e) => void togglePause(e.target.checked)}
          />
        ) : community.error ? (
          <p className="ss-na">
            {t('settings.invites.pauseLoadFailed', { error: community.error })}
          </p>
        ) : (
          <p className="ss-na">{t('settings.invites.loadingSettings')}</p>
        )}
        <ActionStatus error={pause.error} />
      </div>
      <ActionStatus error={action.error} />
      <Async res={res} label={t('settings.invites.loading')}>
        {(invites) =>
          invites.length === 0 ? (
            <EmptyState
              title={t('settings.invites.emptyTitle')}
              description={t('settings.invites.emptyDescription')}
            />
          ) : (
            <table className="ss-table">
              <thead>
                <tr>
                  <th scope="col">{t('settings.invites.colInviter')}</th>
                  <th scope="col">{t('settings.invites.colCode')}</th>
                  <th scope="col">{t('settings.invites.colUses')}</th>
                  <th scope="col">{t('settings.invites.colExpires')}</th>
                  <th scope="col">{t('settings.invites.colChannel')}</th>
                  <th scope="col">
                    <span className="ss-sr">{t('settings.invites.colActions')}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {invites.map((i) => (
                  <tr key={i.code}>
                    <td>
                      {api.userName(
                        i.inviter as api.UserRef | undefined,
                        t('settings.invites.unknown'),
                      )}
                    </td>
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
                          aria-label={t('settings.invites.copyFor', { code: i.code })}
                          onClick={() => void copy(i.code)}
                        >
                          {copied === i.code
                            ? t('settings.invites.copied')
                            : t('settings.invites.copyLink')}
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('settings.invites.revokeFor', { code: i.code })}
                          onClick={() => void revoke(i.code)}
                        >
                          {t('settings.invites.revoke')}
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
