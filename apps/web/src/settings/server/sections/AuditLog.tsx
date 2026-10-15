import { Button, EmptyState } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../../../i18n';
import { snowflakeTime } from '../../../lounge/liveMap';
import { useChannels, useGuild, useMembers } from '../../../store';
import * as api from '../api';
import { AUDIT_FILTERS, actorLabel, describeEntry, diffLines, type Names } from '../auditText';
import { useGuildEvents } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Loading, SectionHeader, formatDateTime } from '../ui';

export function AuditLogSection({ guildId }: SectionProps) {
  const t = useT();
  const { roles } = useGuild(guildId);
  const channels = useChannels(guildId);
  const { members } = useMembers(guildId);
  const [userId, setUserId] = useState('');
  const [action, setAction] = useState('');
  const [entries, setEntries] = useState<api.AuditEntry[]>([]);
  const [users, setUsers] = useState<Record<string, api.UserRef>>({});
  const [loading, setLoading] = useState(true);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const seq = useRef(0);

  const load = useCallback(
    async (append: boolean, before?: string) => {
      const mine = ++seq.current;
      setLoading(true);
      try {
        const res = await api.listAuditLog(guildId, {
          ...(userId ? { user_id: userId } : {}),
          ...(action ? { action_type: Number(action) } : {}),
          ...(before ? { before } : {}),
        });
        if (mine !== seq.current) return;
        setEntries((cur) => (append ? [...cur, ...res.audit_log_entries] : res.audit_log_entries));
        setUsers((cur) => {
          const next = append ? { ...cur } : {};
          for (const u of res.users) next[u.id] = u;
          return next;
        });
        setMore(res.audit_log_entries.length >= api.AUDIT_PAGE);
        setError(null);
      } catch (e) {
        if (mine === seq.current) setError(api.errorText(e, t('settings.auditLog.loadFailed')));
      } finally {
        if (mine === seq.current) setLoading(false);
      }
    },
    [guildId, userId, action, t],
  );

  useEffect(() => {
    void load(false);
  }, [load]);
  // New entries appear at the top: refetch the first page when a guild mutation event arrives.
  useGuildEvents(
    guildId,
    [
      'GUILD_UPDATE',
      'GUILD_BAN_ADD',
      'GUILD_BAN_REMOVE',
      'GUILD_ROLE_CREATE',
      'GUILD_ROLE_DELETE',
      'CHANNEL_CREATE',
      'CHANNEL_DELETE',
    ],
    () => void load(false),
  );

  const userName = (id: string) => {
    const u = users[id];
    if (u) return api.userName(u);
    const m = members.find((x) => x.member.user_id === id);
    return m?.user
      ? m.user.global_name || m.user.username
      : t('settings.auditLog.userFallback', { id });
  };
  const names: Names = {
    user: userName,
    role: (id) =>
      roles.find((r) => r.id === id)?.name ?? t('settings.auditLog.roleFallback', { id }),
    channel: (id) =>
      channels.find((c) => c.id === id)?.name ?? t('settings.auditLog.channelFallback', { id }),
  };
  const memberOptions = members
    .map((m) => ({
      id: m.member.user_id,
      name: m.member.nick || m.user?.global_name || m.user?.username || m.member.user_id,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));

  return (
    <>
      <SectionHeader
        title={t('settings.auditLog.title')}
        description={t('settings.auditLog.description')}
      />
      <div className="ss-toolbar">
        <div>
          <label className="ss-field-label" htmlFor="ss-audit-user">
            {t('settings.auditLog.filterUser')}
          </label>
          <select
            id="ss-audit-user"
            className="ss-select"
            value={userId}
            onChange={(e) => setUserId(e.target.value)}
          >
            <option value="">{t('settings.auditLog.allUsers')}</option>
            {memberOptions.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-audit-action">
            {t('settings.auditLog.filterAction')}
          </label>
          <select
            id="ss-audit-action"
            className="ss-select"
            value={action}
            onChange={(e) => setAction(e.target.value)}
          >
            <option value="">{t('settings.auditLog.allActions')}</option>
            {AUDIT_FILTERS.map((f) => (
              <option key={f.value} value={f.value}>
                {f.label}
              </option>
            ))}
          </select>
        </div>
      </div>
      {error && <ActionStatus error={error} />}
      {loading && entries.length === 0 && <Loading label={t('settings.auditLog.loading')} />}
      {!loading && !error && entries.length === 0 && (
        <EmptyState
          title={t('settings.auditLog.emptyTitle')}
          description={t('settings.auditLog.emptyDescription')}
        />
      )}
      <ul className="ss-list" aria-label={t('settings.auditLog.entriesLabel')}>
        {entries.map((e) => {
          const diff = diffLines(e.changes);
          const expanded = open === e.id;
          return (
            <li key={e.id} className="ss-row" style={{ alignItems: 'flex-start' }}>
              <div className="ss-grow">
                <div className="ss-name" style={{ whiteSpace: 'normal' }}>
                  {describeEntry(e, actorLabel(e, userName(e.user_id)), names)}
                </div>
                <div className="ss-meta">
                  {formatDateTime(snowflakeTime(e.id))}
                  {e.reason ? t('settings.auditLog.reason', { reason: e.reason }) : ''}
                </div>
                {expanded && (
                  <ul className="ss-diff" aria-label={t('settings.auditLog.changesLabel')}>
                    {diff.length === 0 && (
                      <li className="ss-meta">{t('settings.auditLog.noChanges')}</li>
                    )}
                    {diff.map((d) => (
                      <li key={d.key}>
                        <strong>{d.label}</strong>:{' '}
                        {d.before !== null && <span className="ss-old">{d.before}</span>}
                        {d.before !== null && d.after !== null && ' → '}
                        {d.after !== null && <span className="ss-new">{d.after}</span>}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              <Button
                size="sm"
                variant="ghost"
                aria-expanded={expanded}
                aria-label={
                  expanded
                    ? t('settings.auditLog.hideDetails', { id: e.id })
                    : t('settings.auditLog.showDetails', { id: e.id })
                }
                onClick={() => setOpen(expanded ? null : e.id)}
              >
                {expanded ? t('settings.auditLog.hide') : t('settings.auditLog.details')}
              </Button>
            </li>
          );
        })}
      </ul>
      {more && (
        <div className="ss-pager">
          <Button
            variant="secondary"
            loading={loading}
            onClick={() => void load(true, entries.at(-1)?.id)}
          >
            {t('settings.auditLog.loadMore')}
          </Button>
        </div>
      )}
    </>
  );
}
