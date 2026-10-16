import { Avatar, Button, Checkbox, Modal, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { formatDateTime, useT } from '../../../i18n';
import { useGuild } from '../../../store';
import { onDispatch } from '../../../store/events';
import { BulkDialog, type BulkKind, type BulkResult } from './MemberBulk';
import { PruneDialog } from './PruneDialog';
import * as api from '../api';
import type { Viewer } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import {
  ActionStatus,
  ColorDot,
  Loading,
  LoadError,
  SectionHeader,
  formatDate,
  useAction,
} from '../ui';

const VIEW_PAGE = 25;

/** `label` is a message key, resolved at render time. */
export const DELETE_WINDOWS = [
  { seconds: 0, label: 'settings.members.deleteNone' },
  { seconds: 86400, label: 'settings.members.delete24h' },
  { seconds: 604800, label: 'settings.members.delete7d' },
];
/** `label` is a message key, resolved at render time. */
export const TIMEOUT_CHOICES = [
  { seconds: 60, label: 'settings.members.timeout60s' },
  { seconds: 300, label: 'settings.members.timeout5m' },
  { seconds: 600, label: 'settings.members.timeout10m' },
  { seconds: 3600, label: 'settings.members.timeout1h' },
  { seconds: 86400, label: 'settings.members.timeout1d' },
  { seconds: 604800, label: 'settings.members.timeout1w' },
];

const memberName = (m: api.MemberObject) => m.nick || api.userName(m.user);

type SortKey = 'name' | 'newest' | 'oldest';

type Dialog = { kind: 'kick' | 'ban' | 'timeout' | 'roles'; member: api.MemberObject } | null;

export function MembersSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const { guild, roles } = useGuild(guildId);
  const [members, setMembers] = useState<api.MemberObject[]>([]);
  const [loading, setLoading] = useState(true);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  /** `query` after a short pause: this is what the server is asked for. */
  const [applied, setApplied] = useState('');
  const [roleFilter, setRoleFilter] = useState('');
  const [unusual, setUnusual] = useState(false);
  /** `YYYY-MM-DD` from the date inputs; empty means no bound. Applied here to the loaded members. */
  const [joinedAfter, setJoinedAfter] = useState('');
  const [joinedBefore, setJoinedBefore] = useState('');
  const [page, setPage] = useState(0);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [details, setDetails] = useState<api.MemberObject | null>(null);
  const [pruning, setPruning] = useState(false);
  const [pruned, setPruned] = useState<number | null>(null);
  const [sort, setSort] = useState<SortKey>('name');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulk, setBulk] = useState<BulkKind | null>(null);
  const [bulkResult, setBulkResult] = useState<BulkResult | null>(null);
  const busy = useRef(false);
  /** Bumped when the filters change so a response for an older filter is dropped. */
  const generation = useRef(0);

  useEffect(() => {
    const timer = setTimeout(() => setApplied(query.trim()), 250);
    return () => clearTimeout(timer);
  }, [query]);

  const loadNext = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    const mine = generation.current;
    setLoading(true);
    try {
      const cursor = membersRef.current.at(-1)?.user.id;
      const batch = await api.listMembers(guildId, {
        ...(cursor ? { after: cursor } : {}),
        ...(applied ? { query: applied } : {}),
        ...(roleFilter ? { roleId: roleFilter } : {}),
        ...(unusual ? { signals: true } : {}),
      });
      if (mine !== generation.current) return;
      setMembers((cur) => {
        const seen = new Set(cur.map((m) => m.user.id));
        return [...cur, ...batch.filter((m) => !seen.has(m.user.id))];
      });
      if (batch.length < api.MEMBER_PAGE) setDone(true);
      setError(null);
    } catch (e) {
      if (mine === generation.current) setError(api.errorText(e, t('settings.members.loadFailed')));
    } finally {
      if (mine === generation.current) {
        busy.current = false;
        setLoading(false);
      }
    }
  }, [guildId, applied, roleFilter, unusual, t]);
  const membersRef = useRef(members);
  membersRef.current = members;

  /** Start over from the first page (filters changed, or after a prune). */
  const restart = useCallback(() => {
    generation.current += 1;
    busy.current = false;
    membersRef.current = [];
    setMembers([]);
    setDone(false);
    setPage(0);
    void loadNext();
  }, [loadNext]);

  // First page, and again from the start whenever the search or the role filter changes.
  useEffect(() => {
    restart();
  }, [restart]);

  // Live updates from the gateway.
  useEffect(
    () =>
      onDispatch((event, data) => {
        const d = data as {
          guild_id?: string;
          user?: api.UserRef;
          roles?: string[];
          nick?: string | null;
        } | null;
        if (!d || d.guild_id !== guildId || !d.user) return;
        const uid = d.user.id;
        if (event === 'GUILD_MEMBER_REMOVE')
          setMembers((cur) => cur.filter((m) => m.user.id !== uid));
        else if (event === 'GUILD_MEMBER_UPDATE')
          setMembers((cur) =>
            cur.map((m) =>
              m.user.id === uid
                ? {
                    ...m,
                    user: { ...m.user, ...d.user },
                    roles: d.roles ?? m.roles,
                    nick: d.nick === undefined ? m.nick : d.nick,
                    communication_disabled_until:
                      (data as { communication_disabled_until?: string | null })
                        .communication_disabled_until ?? m.communication_disabled_until,
                  }
                : m,
            ),
          );
      }),
    [guildId],
  );

  const roleById = useMemo(() => new Map(roles.map((r) => [r.id, r])), [roles]);
  // The server did the filtering (search text, role); the order is applied here to the members loaded so far.
  const filtered = useMemo(() => {
    // Bounds are whole local days, both inclusive.
    const from = joinedAfter ? new Date(`${joinedAfter}T00:00:00`).getTime() : NaN;
    const to = joinedBefore ? new Date(`${joinedBefore}T23:59:59.999`).getTime() : NaN;
    const out = members.filter((m) => {
      if (Number.isNaN(from) && Number.isNaN(to)) return true;
      const at = new Date(m.joined_at).getTime();
      if (Number.isNaN(at)) return false;
      return (Number.isNaN(from) || at >= from) && (Number.isNaN(to) || at <= to);
    });
    if (sort === 'name')
      out.sort((a, b) =>
        memberName(a).localeCompare(memberName(b), undefined, { sensitivity: 'base' }),
      );
    else {
      const at = (m: api.MemberObject) => new Date(m.joined_at).getTime() || 0;
      out.sort((a, b) => (sort === 'newest' ? at(b) - at(a) : at(a) - at(b)));
    }
    return out;
  }, [members, sort, joinedAfter, joinedBefore]);
  const pageCount = Math.max(1, Math.ceil(filtered.length / VIEW_PAGE));
  const safePage = Math.min(page, pageCount - 1);
  const visible = filtered.slice(safePage * VIEW_PAGE, (safePage + 1) * VIEW_PAGE);

  const topOf = (m: api.MemberObject) =>
    Math.max(0, ...m.roles.map((r) => roleById.get(r)?.position ?? 0));
  /** Why the viewer cannot moderate this member, or undefined when they can. */
  const blockReason = (m: api.MemberObject): string | undefined => {
    if (m.user.id === guild?.owner_id) return t('settings.members.blockOwner');
    if (m.user.id === viewer.userId) return t('settings.members.blockSelf');
    if (!viewer.isOwner && topOf(m) >= viewer.topPosition) return t('settings.members.blockRole');
    return undefined;
  };

  const selectedMembers = members.filter((m) => selected.has(m.user.id));
  /** Kick and ban skip members the viewer cannot moderate; roles are checked by the server. */
  const bulkTargets = (kind: BulkKind) =>
    kind === 'kick' || kind === 'ban'
      ? selectedMembers.filter((m) => !blockReason(m))
      : selectedMembers;
  const toggleOne = (id: string, on: boolean) =>
    setSelected((cur) => {
      const next = new Set(cur);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  const pageIds = visible.map((m) => m.user.id);
  const allOnPage = pageIds.length > 0 && pageIds.every((id) => selected.has(id));
  const someOnPage = pageIds.some((id) => selected.has(id));
  const toggleAll = (on: boolean) =>
    setSelected((cur) => {
      const next = new Set(cur);
      for (const id of pageIds) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  const assignable = roles
    .filter((r) => r.id !== guildId && (viewer.isOwner || r.position < viewer.topPosition))
    .sort((a, b) => b.position - a.position);
  const bulkApplied = (r: BulkResult) => {
    if (r.roleId) {
      const rid = r.roleId;
      setMembers((cur) =>
        cur.map((m) =>
          r.ok.includes(m.user.id)
            ? {
                ...m,
                roles:
                  r.kind === 'addRole'
                    ? m.roles.includes(rid)
                      ? m.roles
                      : [...m.roles, rid]
                    : m.roles.filter((x) => x !== rid),
              }
            : m,
        ),
      );
    } else setMembers((cur) => cur.filter((m) => !r.ok.includes(m.user.id)));
    setBulk(null);
    setBulkResult(r);
    // Keep the failures selected so the action can be retried.
    setSelected(new Set(r.failed));
  };

  const replace = (next: api.MemberObject) =>
    setMembers((cur) => cur.map((m) => (m.user.id === next.user.id ? next : m)));
  const drop = (id: string) => setMembers((cur) => cur.filter((m) => m.user.id !== id));

  return (
    <>
      <SectionHeader
        title={t('settings.members.title')}
        description={t('settings.members.description')}
      />
      <div className="ss-toolbar">
        <div className="ss-search">
          <TextInput
            label={t('settings.members.searchLabel')}
            type="search"
            value={query}
            placeholder={t('settings.members.searchPlaceholder')}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-member-role">
            {t('settings.members.filterRole')}
          </label>
          <select
            id="ss-member-role"
            className="ss-select"
            value={roleFilter}
            onChange={(e) => setRoleFilter(e.target.value)}
          >
            <option value="">{t('settings.members.allRoles')}</option>
            {[...roles]
              .filter((r) => r.id !== guildId)
              .sort((a, b) => b.position - a.position)
              .map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                </option>
              ))}
          </select>
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-member-after">
            {t('settings.members.joinedAfter')}
          </label>
          <input
            id="ss-member-after"
            type="date"
            className="ss-select"
            value={joinedAfter}
            max={joinedBefore || undefined}
            onChange={(e) => {
              setJoinedAfter(e.target.value);
              setPage(0);
            }}
          />
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-member-before">
            {t('settings.members.joinedBefore')}
          </label>
          <input
            id="ss-member-before"
            type="date"
            className="ss-select"
            value={joinedBefore}
            min={joinedAfter || undefined}
            onChange={(e) => {
              setJoinedBefore(e.target.value);
              setPage(0);
            }}
          />
        </div>
        {viewer.can('MANAGE_GUILD') && (
          <Checkbox
            label={t('settings.members.unusual')}
            checked={unusual}
            onChange={(e) => setUnusual(e.target.checked)}
          />
        )}
        <div>
          <label className="ss-field-label" htmlFor="ss-member-sort">
            {t('settings.members.sortBy')}
          </label>
          <select
            id="ss-member-sort"
            className="ss-select"
            value={sort}
            onChange={(e) => {
              setSort(e.target.value as SortKey);
              setPage(0);
            }}
          >
            <option value="name">{t('settings.members.sortName')}</option>
            <option value="newest">{t('settings.members.sortNewest')}</option>
            <option value="oldest">{t('settings.members.sortOldest')}</option>
          </select>
        </div>
        {viewer.can('KICK_MEMBERS') && (
          <Button variant="secondary" onClick={() => setPruning(true)}>
            {t('settings.members.prune')}
          </Button>
        )}
      </div>
      {pruned !== null && (
        <p role="status" className="ss-sub">
          {t('settings.members.pruned', { count: pruned })}
        </p>
      )}
      {bulkResult && (
        <p role="status" className="ss-sub">
          {bulkResult.failed.length === 0
            ? t('settings.members.bulkDone', { count: bulkResult.ok.length })
            : t('settings.members.bulkPartial', {
                ok: bulkResult.ok.length,
                failed: bulkResult.failed.length,
              })}
        </p>
      )}
      {selected.size > 0 && (
        <div className="ss-toolbar" role="toolbar" aria-label={t('settings.members.bulkBar')}>
          <span>{t('settings.members.selected', { count: selected.size })}</span>
          {viewer.can('MANAGE_ROLES') && (
            <>
              <Button size="sm" variant="secondary" onClick={() => setBulk('addRole')}>
                {t('settings.members.bulkAddRole')}
              </Button>
              <Button size="sm" variant="secondary" onClick={() => setBulk('removeRole')}>
                {t('settings.members.bulkRemoveRole')}
              </Button>
            </>
          )}
          {viewer.can('KICK_MEMBERS') && (
            <Button size="sm" variant="danger" onClick={() => setBulk('kick')}>
              {t('settings.members.kick')}
            </Button>
          )}
          {viewer.can('BAN_MEMBERS') && (
            <Button size="sm" variant="danger" onClick={() => setBulk('ban')}>
              {t('settings.members.ban')}
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
            {t('settings.members.clearSelection')}
          </Button>
        </div>
      )}
      {error && <LoadError message={error} onRetry={() => void loadNext()} />}
      {loading && members.length === 0 && !error && (
        <Loading label={t('settings.members.loading')} />
      )}
      {members.length === 0 && done && !loading && !error && (
        <p className="ss-na">{t('settings.members.noMatch')}</p>
      )}
      {members.length > 0 && (
        <>
          <table className="ss-table">
            <caption className="ss-sub">
              {done
                ? t('settings.members.count', { count: filtered.length })
                : t('settings.members.countLoaded', { count: filtered.length })}
            </caption>
            <thead>
              <tr>
                <th scope="col">
                  <input
                    type="checkbox"
                    aria-label={t('settings.members.selectAll')}
                    checked={allOnPage}
                    ref={(el) => {
                      if (el) el.indeterminate = !allOnPage && someOnPage;
                    }}
                    onChange={(e) => toggleAll(e.target.checked)}
                  />
                </th>
                <th scope="col">{t('settings.members.colName')}</th>
                <th scope="col">{t('settings.members.colSince')}</th>
                {viewer.can('MANAGE_GUILD') && (
                  <>
                    <th scope="col">{t('settings.members.colJoined')}</th>
                    <th scope="col">{t('settings.members.colSignals')}</th>
                  </>
                )}
                <th scope="col">{t('settings.members.colRoles')}</th>
                <th scope="col">{t('settings.members.colActions')}</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((m) => (
                <MemberRow
                  key={m.user.id}
                  member={m}
                  roleById={roleById}
                  viewer={viewer}
                  block={blockReason(m)}
                  checked={selected.has(m.user.id)}
                  onCheck={(on) => toggleOne(m.user.id, on)}
                  onAction={(kind) => setDialog({ kind, member: m })}
                  onDetails={() => setDetails(m)}
                />
              ))}
            </tbody>
          </table>
          <div className="ss-pager">
            <Button
              size="sm"
              variant="secondary"
              disabled={safePage === 0}
              onClick={() => setPage(safePage - 1)}
            >
              {t('settings.members.previous')}
            </Button>
            <span>{t('settings.members.page', { page: safePage + 1, pages: pageCount })}</span>
            <Button
              size="sm"
              variant="secondary"
              disabled={safePage >= pageCount - 1}
              onClick={() => setPage(safePage + 1)}
            >
              {t('settings.members.next')}
            </Button>
            {!done && (
              <Button size="sm" variant="ghost" loading={loading} onClick={() => void loadNext()}>
                {t('settings.members.loadMore')}
              </Button>
            )}
          </div>
        </>
      )}
      {bulk && (
        <BulkDialog
          guildId={guildId}
          kind={bulk}
          members={bulkTargets(bulk)}
          assignable={assignable}
          onClose={() => setBulk(null)}
          onDone={bulkApplied}
        />
      )}
      {pruning && (
        <PruneDialog
          guildId={guildId}
          roles={roles}
          onClose={() => setPruning(false)}
          onDone={(n) => {
            setPruning(false);
            setPruned(n);
            // The gateway removes each pruned member from the list; reload covers a missed event.
            restart();
          }}
        />
      )}
      {details && (
        <MemberDetails
          member={members.find((m) => m.user.id === details.user.id) ?? details}
          roleById={roleById}
          onClose={() => setDetails(null)}
        />
      )}
      {dialog?.kind === 'kick' && (
        <KickDialog
          guildId={guildId}
          member={dialog.member}
          onClose={() => setDialog(null)}
          onDone={() => {
            drop(dialog.member.user.id);
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'ban' && (
        <BanDialog
          guildId={guildId}
          member={dialog.member}
          onClose={() => setDialog(null)}
          onDone={() => {
            drop(dialog.member.user.id);
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'timeout' && (
        <TimeoutDialog
          guildId={guildId}
          member={dialog.member}
          onClose={() => setDialog(null)}
          onDone={(m) => {
            replace(m);
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'roles' && (
        <MemberRolesDialog
          guildId={guildId}
          viewer={viewer}
          member={members.find((m) => m.user.id === dialog.member.user.id) ?? dialog.member}
          roles={roles}
          onChange={replace}
          onClose={() => setDialog(null)}
        />
      )}
    </>
  );
}

/** "Joined via": the source, with the code for an invite or a vanity link (D-240). */
function joinLabel(m: api.MemberObject, t: ReturnType<typeof useT>): string {
  const j = m.sr_join;
  if (!j) return '';
  switch (j.source) {
    case 'invite':
      return j.invite_code
        ? t('settings.members.joinInviteCode', { code: j.invite_code })
        : t('settings.members.joinInvite');
    case 'vanity':
      return t('settings.members.joinVanity', { code: j.invite_code ?? '' });
    case 'directory':
      return t('settings.members.joinDirectory');
    case 'application':
      return t('settings.members.joinApplication');
    case 'creator':
      return t('settings.members.joinCreator');
    case 'staff':
      return t('settings.members.joinStaff');
    case 'bot':
      return t('settings.members.joinBot');
    default:
      return t('settings.members.joinUnknown');
  }
}

function MemberRow({
  member,
  roleById,
  viewer,
  block,
  checked,
  onCheck,
  onAction,
  onDetails,
}: {
  checked: boolean;
  onCheck: (on: boolean) => void;
  member: api.MemberObject;
  roleById: Map<string, { id: string; name: string; color: number }>;
  viewer: Viewer;
  block: string | undefined;
  onAction: (k: 'kick' | 'ban' | 'timeout' | 'roles') => void;
  onDetails: () => void;
}) {
  const t = useT();
  const name = memberName(member);
  const timedOut =
    member.communication_disabled_until &&
    new Date(member.communication_disabled_until).getTime() > Date.now();
  const btn = (
    kind: 'kick' | 'ban' | 'timeout' | 'roles',
    label: string,
    perm: string,
    danger = false,
  ) => {
    const noPerm = !viewer.can(perm);
    const why = noPerm
      ? t('settings.members.needPermission', { label })
      : kind === 'roles'
        ? undefined
        : block;
    return (
      <Button
        size="sm"
        variant={danger ? 'danger' : 'secondary'}
        disabled={!!why}
        title={why}
        aria-label={t('settings.members.actionFor', { label, name })}
        onClick={() => onAction(kind)}
      >
        {label}
      </Button>
    );
  };
  return (
    <tr>
      <td>
        <input
          type="checkbox"
          aria-label={t('settings.members.selectMember', { name })}
          checked={checked}
          onChange={(e) => onCheck(e.target.checked)}
        />
      </td>
      <td>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
          <Avatar name={name} size={32} />
          <span>
            <button
              type="button"
              className="ss-link"
              aria-label={t('settings.members.detailsFor', { name })}
              onClick={onDetails}
            >
              <strong>{name}</strong>
            </button>
            {member.nick && <span className="ss-meta"> ({api.userName(member.user)})</span>}
            {timedOut && <span className="ss-meta"> {t('settings.members.timedOut')}</span>}
          </span>
        </span>
      </td>
      <td>{formatDate(member.joined_at)}</td>
      {viewer.can('MANAGE_GUILD') && (
        <>
          <td>{joinLabel(member, t)}</td>
          <td>
            <span className="ss-chips">
              {(member.sr_signals ?? []).map((sig) => (
                <span key={sig} className="ss-role-chip">
                  {t(
                    sig === 'rejoined'
                      ? 'settings.members.signalRejoined'
                      : 'settings.members.signalNew',
                  )}
                </span>
              ))}
            </span>
          </td>
        </>
      )}
      <td>
        <span className="ss-chips">
          {member.roles.map((rid) => {
            const r = roleById.get(rid);
            return r ? (
              <span key={rid} className="ss-role-chip">
                <ColorDot color={r.color} />
                {r.name}
              </span>
            ) : null;
          })}
        </span>
      </td>
      <td>
        <span className="ss-actions">
          {btn('roles', t('settings.members.roles'), 'MANAGE_ROLES')}
          {btn('timeout', t('settings.members.timeout'), 'MODERATE_MEMBERS')}
          {btn('kick', t('settings.members.kick'), 'KICK_MEMBERS', true)}
          {btn('ban', t('settings.members.ban'), 'BAN_MEMBERS', true)}
        </span>
      </td>
    </tr>
  );
}

function KickDialog({
  guildId,
  member,
  onClose,
  onDone,
}: {
  guildId: string;
  member: api.MemberObject;
  onClose: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const [reason, setReason] = useState('');
  const action = useAction();
  const submit = async () => {
    const ok = await action.run(async () => {
      await api.kickMember(guildId, member.user.id, reason.trim() || undefined);
      return true;
    });
    if (ok) onDone();
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.members.kickTitle', { name: memberName(member) })}
      description={t('settings.members.kickDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.members.cancel')}
          </Button>
          <Button variant="danger" loading={action.busy} onClick={() => void submit()}>
            {t('settings.members.kick')}
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <TextArea
        label={t('settings.members.reason')}
        value={reason}
        maxLength={512}
        onChange={(e) => setReason(e.target.value)}
      />
    </Modal>
  );
}

function BanDialog({
  guildId,
  member,
  onClose,
  onDone,
}: {
  guildId: string;
  member: api.MemberObject;
  onClose: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const [reason, setReason] = useState('');
  const [seconds, setSeconds] = useState(0);
  const action = useAction();
  const submit = async () => {
    const ok = await action.run(async () => {
      await api.banMember(guildId, member.user.id, {
        ...(reason.trim() ? { reason: reason.trim() } : {}),
        delete_message_seconds: seconds,
      });
      return true;
    });
    if (ok) onDone();
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.members.banTitle', { name: memberName(member) })}
      description={t('settings.members.banDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.members.cancel')}
          </Button>
          <Button variant="danger" loading={action.busy} onClick={() => void submit()}>
            {t('settings.members.ban')}
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <div>
          <label className="ss-field-label" htmlFor="ss-ban-delete">
            {t('settings.members.deleteHistory')}
          </label>
          <select
            id="ss-ban-delete"
            className="ss-select"
            value={seconds}
            onChange={(e) => setSeconds(Number(e.target.value))}
          >
            {DELETE_WINDOWS.map((w) => (
              <option key={w.seconds} value={w.seconds}>
                {t(w.label)}
              </option>
            ))}
          </select>
        </div>
        <TextArea
          label={t('settings.members.reason')}
          value={reason}
          maxLength={512}
          onChange={(e) => setReason(e.target.value)}
        />
      </div>
    </Modal>
  );
}

function TimeoutDialog({
  guildId,
  member,
  onClose,
  onDone,
}: {
  guildId: string;
  member: api.MemberObject;
  onClose: () => void;
  onDone: (m: api.MemberObject) => void;
}) {
  const t = useT();
  const [seconds, setSeconds] = useState(600);
  const action = useAction();
  const active =
    !!member.communication_disabled_until &&
    new Date(member.communication_disabled_until).getTime() > Date.now();
  const apply = async (until: string | null) => {
    const m = await action.run(() => api.timeoutMember(guildId, member.user.id, until));
    if (m) onDone({ ...member, ...m, user: { ...member.user, ...m.user } });
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.members.timeoutTitle', { name: memberName(member) })}
      description={t('settings.members.timeoutDescription')}
      footer={
        <>
          {active && (
            <Button variant="secondary" loading={action.busy} onClick={() => void apply(null)}>
              {t('settings.members.removeTimeout')}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t('settings.members.cancel')}
          </Button>
          <Button
            loading={action.busy}
            onClick={() => void apply(new Date(Date.now() + seconds * 1000).toISOString())}
          >
            {t('settings.members.timeout')}
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <label className="ss-field-label" htmlFor="ss-timeout-len">
        {t('settings.members.duration')}
      </label>
      <select
        id="ss-timeout-len"
        className="ss-select"
        value={seconds}
        onChange={(e) => setSeconds(Number(e.target.value))}
      >
        {TIMEOUT_CHOICES.map((c) => (
          <option key={c.seconds} value={c.seconds}>
            {t(c.label)}
          </option>
        ))}
      </select>
    </Modal>
  );
}

function MemberRolesDialog({
  guildId,
  viewer,
  member,
  roles,
  onChange,
  onClose,
}: {
  guildId: string;
  viewer: Viewer;
  member: api.MemberObject;
  roles: { id: string; name: string; color: number; position: number }[];
  onChange: (m: api.MemberObject) => void;
  onClose: () => void;
}) {
  const t = useT();
  const action = useAction();
  const assignable = roles.filter((r) => r.id !== guildId).sort((a, b) => b.position - a.position);
  const toggle = async (roleId: string, on: boolean) => {
    const ok = await action.run(async () => {
      if (on) await api.addMemberRole(guildId, member.user.id, roleId);
      else await api.removeMemberRole(guildId, member.user.id, roleId);
      return true;
    });
    if (ok)
      onChange({
        ...member,
        roles: on ? [...member.roles, roleId] : member.roles.filter((r) => r !== roleId),
      });
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.members.rolesFor', { name: memberName(member) })}
    >
      <ActionStatus error={action.error} />
      <ul className="ss-list" aria-label={t('settings.members.roles')}>
        {assignable.map((r) => {
          const why =
            !viewer.isOwner && viewer.topPosition <= r.position
              ? t('settings.members.aboveHighest')
              : undefined;
          return (
            <li key={r.id} className="ss-row">
              <Checkbox
                label={r.name}
                description={why}
                checked={member.roles.includes(r.id)}
                disabled={!!why || action.busy}
                onChange={(e) => void toggle(r.id, e.target.checked)}
              />
            </li>
          );
        })}
      </ul>
    </Modal>
  );
}

function MemberDetails({
  member,
  roleById,
  onClose,
}: {
  member: api.MemberObject;
  roleById: Map<string, { id: string; name: string; color: number }>;
  onClose: () => void;
}) {
  const t = useT();
  const name = memberName(member);
  const until = member.communication_disabled_until;
  const timedOut = !!until && new Date(until).getTime() > Date.now();
  return (
    <Modal open onClose={onClose} size="sm" title={name}>
      <dl className="ss-details">
        <div>
          <dt>{t('settings.members.username')}</dt>
          <dd>{member.user.username ?? t('settings.members.unknown')}</dd>
        </div>
        <div>
          <dt>{t('settings.members.userId')}</dt>
          <dd>
            <code>{member.user.id}</code>
          </dd>
        </div>
        <div>
          <dt>{t('settings.members.colSince')}</dt>
          <dd>{formatDate(member.joined_at)}</dd>
        </div>
        {member.nick && (
          <div>
            <dt>{t('settings.members.nickname')}</dt>
            <dd>{member.nick}</dd>
          </div>
        )}
        <div>
          <dt>{t('settings.members.timeout')}</dt>
          <dd>
            {timedOut && until
              ? t('settings.members.until', { when: formatDateTime(new Date(until)) })
              : t('settings.members.notTimedOut')}
          </dd>
        </div>
        <div>
          <dt>{t('settings.members.roles')}</dt>
          <dd>
            <span className="ss-chips">
              {member.roles.map((rid) => {
                const r = roleById.get(rid);
                return r ? (
                  <span key={rid} className="ss-role-chip">
                    <ColorDot color={r.color} />
                    {r.name}
                  </span>
                ) : null;
              })}
              {member.roles.length === 0 && (
                <span className="ss-meta">{t('settings.members.noRoles')}</span>
              )}
            </span>
          </dd>
        </div>
      </dl>
    </Modal>
  );
}
