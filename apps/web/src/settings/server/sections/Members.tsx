import { Avatar, Button, Checkbox, Modal, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useGuild } from '../../../store';
import { onDispatch } from '../../../store/events';
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
const MAX_PAGES = 20;

export const DELETE_WINDOWS = [
  { seconds: 0, label: "Don't delete any" },
  { seconds: 86400, label: 'Previous 24 hours' },
  { seconds: 604800, label: 'Previous 7 days' },
];
export const TIMEOUT_CHOICES = [
  { seconds: 60, label: '60 secs' },
  { seconds: 300, label: '5 mins' },
  { seconds: 600, label: '10 mins' },
  { seconds: 3600, label: '1 hour' },
  { seconds: 86400, label: '1 day' },
  { seconds: 604800, label: '1 week' },
];

const memberName = (m: api.MemberObject) => m.nick || api.userName(m.user);

type Dialog = { kind: 'kick' | 'ban' | 'timeout' | 'roles'; member: api.MemberObject } | null;

export function MembersSection({ guildId, viewer }: SectionProps) {
  const { guild, roles } = useGuild(guildId);
  const [members, setMembers] = useState<api.MemberObject[]>([]);
  const [loading, setLoading] = useState(true);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [page, setPage] = useState(0);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [pruning, setPruning] = useState(false);
  const [pruned, setPruned] = useState<number | null>(null);
  const pages = useRef(0);
  const busy = useRef(false);

  const loadNext = useCallback(async () => {
    if (busy.current || pages.current >= MAX_PAGES) return;
    busy.current = true;
    setLoading(true);
    try {
      const cursor = membersRef.current.at(-1)?.user.id;
      const batch = await api.listMembers(guildId, cursor);
      pages.current += 1;
      setMembers((cur) => {
        const seen = new Set(cur.map((m) => m.user.id));
        return [...cur, ...batch.filter((m) => !seen.has(m.user.id))];
      });
      if (batch.length < api.MEMBER_PAGE) setDone(true);
      setError(null);
    } catch (e) {
      setError(api.errorText(e, 'Could not load members.'));
    } finally {
      busy.current = false;
      setLoading(false);
    }
  }, [guildId]);
  const membersRef = useRef(members);
  membersRef.current = members;

  useEffect(() => {
    void loadNext();
  }, [loadNext]);

  // Searching needs the whole list: keep fetching pages while a query is typed.
  useEffect(() => {
    if (query.trim() && !done && !loading && !error) void loadNext();
  }, [query, done, loading, error, members.length, loadNext]);

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
  const q = query.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      q
        ? members.filter(
            (m) =>
              memberName(m).toLowerCase().includes(q) ||
              (m.user.username ?? '').toLowerCase().includes(q) ||
              m.user.id.includes(q),
          )
        : members,
    [members, q],
  );
  const pageCount = Math.max(1, Math.ceil(filtered.length / VIEW_PAGE));
  const safePage = Math.min(page, pageCount - 1);
  const visible = filtered.slice(safePage * VIEW_PAGE, (safePage + 1) * VIEW_PAGE);

  const topOf = (m: api.MemberObject) =>
    Math.max(0, ...m.roles.map((r) => roleById.get(r)?.position ?? 0));
  /** Why the viewer cannot moderate this member, or undefined when they can. */
  const blockReason = (m: api.MemberObject): string | undefined => {
    if (m.user.id === guild?.owner_id) return 'The server owner cannot be moderated.';
    if (m.user.id === viewer.userId) return 'You cannot moderate yourself.';
    if (!viewer.isOwner && topOf(m) >= viewer.topPosition)
      return 'This member has a role at or above your highest role.';
    return undefined;
  };

  const replace = (next: api.MemberObject) =>
    setMembers((cur) => cur.map((m) => (m.user.id === next.user.id ? next : m)));
  const drop = (id: string) => setMembers((cur) => cur.filter((m) => m.user.id !== id));

  return (
    <>
      <SectionHeader
        title="Members"
        description="Search the member list, manage roles, time members out, kick or ban."
      />
      <div className="ss-toolbar">
        <div className="ss-search">
          <TextInput
            label="Search members"
            type="search"
            value={query}
            placeholder="Search by name or user ID"
            onChange={(e) => {
              setQuery(e.target.value);
              setPage(0);
            }}
          />
        </div>
        {viewer.can('KICK_MEMBERS') && (
          <Button variant="secondary" onClick={() => setPruning(true)}>
            Prune
          </Button>
        )}
      </div>
      {pruned !== null && (
        <p role="status" className="ss-sub">
          Pruned {pruned} {pruned === 1 ? 'member' : 'members'}.
        </p>
      )}
      {error && <LoadError message={error} onRetry={() => void loadNext()} />}
      {loading && members.length === 0 && !error && <Loading label="Loading members" />}
      {members.length > 0 && (
        <>
          <table className="ss-table">
            <caption className="ss-sub">
              {filtered.length} {filtered.length === 1 ? 'member' : 'members'}
              {!done && ' loaded so far'}
            </caption>
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Member since</th>
                <th scope="col">Roles</th>
                <th scope="col">Actions</th>
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
                  onAction={(kind) => setDialog({ kind, member: m })}
                />
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <p className="ss-na">No members match your search.</p>}
          <div className="ss-pager">
            <Button
              size="sm"
              variant="secondary"
              disabled={safePage === 0}
              onClick={() => setPage(safePage - 1)}
            >
              Previous
            </Button>
            <span>
              Page {safePage + 1} of {pageCount}
            </span>
            <Button
              size="sm"
              variant="secondary"
              disabled={safePage >= pageCount - 1}
              onClick={() => setPage(safePage + 1)}
            >
              Next
            </Button>
            {!done && (
              <Button size="sm" variant="ghost" loading={loading} onClick={() => void loadNext()}>
                Load more members
              </Button>
            )}
          </div>
        </>
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
            setMembers([]);
            membersRef.current = [];
            setDone(false);
            pages.current = 0;
            void loadNext();
          }}
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

function MemberRow({
  member,
  roleById,
  viewer,
  block,
  onAction,
}: {
  member: api.MemberObject;
  roleById: Map<string, { id: string; name: string; color: number }>;
  viewer: Viewer;
  block: string | undefined;
  onAction: (k: 'kick' | 'ban' | 'timeout' | 'roles') => void;
}) {
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
    const why = noPerm ? `You need the ${label} permission.` : kind === 'roles' ? undefined : block;
    return (
      <Button
        size="sm"
        variant={danger ? 'danger' : 'secondary'}
        disabled={!!why}
        title={why}
        aria-label={`${label} ${name}`}
        onClick={() => onAction(kind)}
      >
        {label}
      </Button>
    );
  };
  return (
    <tr>
      <td>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
          <Avatar name={name} size={32} />
          <span>
            <strong>{name}</strong>
            {member.nick && <span className="ss-meta"> ({api.userName(member.user)})</span>}
            {timedOut && <span className="ss-meta"> Timed out</span>}
          </span>
        </span>
      </td>
      <td>{formatDate(member.joined_at)}</td>
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
          {btn('roles', 'Roles', 'MANAGE_ROLES')}
          {btn('timeout', 'Timeout', 'MODERATE_MEMBERS')}
          {btn('kick', 'Kick', 'KICK_MEMBERS', true)}
          {btn('ban', 'Ban', 'BAN_MEMBERS', true)}
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
      title={`Kick ${memberName(member)}`}
      description="They can rejoin with a new invite."
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" loading={action.busy} onClick={() => void submit()}>
            Kick
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <TextArea
        label="Reason"
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
      title={`Ban ${memberName(member)}`}
      description="They will not be able to rejoin until you unban them."
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" loading={action.busy} onClick={() => void submit()}>
            Ban
          </Button>
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error} />
        <div>
          <label className="ss-field-label" htmlFor="ss-ban-delete">
            Delete message history
          </label>
          <select
            id="ss-ban-delete"
            className="ss-select"
            value={seconds}
            onChange={(e) => setSeconds(Number(e.target.value))}
          >
            {DELETE_WINDOWS.map((w) => (
              <option key={w.seconds} value={w.seconds}>
                {w.label}
              </option>
            ))}
          </select>
        </div>
        <TextArea
          label="Reason"
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
      title={`Timeout ${memberName(member)}`}
      description="They cannot send messages, react or speak until the timeout ends."
      footer={
        <>
          {active && (
            <Button variant="secondary" loading={action.busy} onClick={() => void apply(null)}>
              Remove timeout
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            loading={action.busy}
            onClick={() => void apply(new Date(Date.now() + seconds * 1000).toISOString())}
          >
            Timeout
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <label className="ss-field-label" htmlFor="ss-timeout-len">
        Duration
      </label>
      <select
        id="ss-timeout-len"
        className="ss-select"
        value={seconds}
        onChange={(e) => setSeconds(Number(e.target.value))}
      >
        {TIMEOUT_CHOICES.map((c) => (
          <option key={c.seconds} value={c.seconds}>
            {c.label}
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
    <Modal open onClose={onClose} size="sm" title={`Roles for ${memberName(member)}`}>
      <ActionStatus error={action.error} />
      <ul className="ss-list" aria-label="Roles">
        {assignable.map((r) => {
          const why =
            !viewer.isOwner && viewer.topPosition <= r.position
              ? 'At or above your highest role.'
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
