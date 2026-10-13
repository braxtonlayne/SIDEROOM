import { Avatar, DropdownMenu, Icon, Modal, usePortalTarget, useFloating } from '@sideroom/ui';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { apiFetch } from '../../api/client';
import { useViewer } from '../../settings/server/hooks';
import {
  useCurrentUser,
  useGuilds,
  useLoungeActions,
  useMembers,
  usePresence,
  useRoles,
  useUsers,
} from '../data';
import { useOpenDm } from '../shell/dm';
import { setMemberRole } from './memberRoles';
import { usePopout, type ProfileTarget } from './popoutStore';
import { accentCss, avatarUrlOf, bannerUrlOf, useProfile, type Profile } from './useProfile';
import './profile.css';

const NOTE_MAX = 256;

function Banner({ profile }: { profile: Profile | null }) {
  const url = profile ? bannerUrlOf(profile) : null;
  const accent = accentCss(profile?.user_profile.accent_color);
  return (
    <div
      className="lng-pop-banner"
      style={{
        ...(accent ? { background: accent } : {}),
        ...(url ? { backgroundImage: `url(${url})` } : {}),
      }}
    />
  );
}

/** Private note about the person, saved when the box loses focus (`GET/PUT /users/@me/notes/{id}`). */
function NoteField({ userId }: { userId: string }) {
  const [note, setNote] = useState('');
  const saved = useRef('');
  useEffect(() => {
    let live = true;
    apiFetch<{ note: string }>(`/users/@me/notes/${encodeURIComponent(userId)}`, { retries: 0 })
      .then((n) => {
        if (!live) return;
        saved.current = n.note;
        setNote(n.note);
      })
      .catch(() => undefined); // 404: no note yet
    return () => {
      live = false;
    };
  }, [userId]);
  const save = () => {
    const next = note.trim();
    if (next === saved.current) return;
    saved.current = next;
    apiFetch<void>(`/users/@me/notes/${encodeURIComponent(userId)}`, {
      method: 'PUT',
      json: { note: next || null },
      retries: 0,
    }).catch(() => undefined);
  };
  return (
    <section aria-labelledby="pop-note-h">
      <h3 id="pop-note-h" className="lng-pop-h">
        Note
      </h3>
      <input
        className="lng-pop-input"
        aria-label="Note (only visible to you)"
        placeholder="Click to add a note"
        maxLength={NOTE_MAX}
        value={note}
        onChange={(e) => setNote(e.target.value)}
        onBlur={save}
        onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      />
    </section>
  );
}

/** The role chips of a member, with remove and add for people who may manage those roles. */
function RoleChips({ target }: { target: ProfileTarget }) {
  const guildId = target.guildId!;
  const member = useMembers(guildId).find((m) => m.user.id === target.userId);
  const roles = useRoles(guildId);
  const viewer = useViewer(guildId);
  const [busy, setBusy] = useState<string | null>(null);
  if (!member) return null;
  const held = roles.filter((r) => r.id !== guildId && member.roleIds.includes(r.id));
  const addable = roles.filter(
    (r) => r.id !== guildId && !member.roleIds.includes(r.id) && viewer.canManageRole(r.position),
  );
  const change = async (roleId: string, on: boolean) => {
    setBusy(roleId);
    try {
      await setMemberRole(
        guildId,
        { userId: member.user.id, nick: member.nick, roleIds: member.roleIds },
        roleId,
        on,
      );
    } catch {
      /* the chip simply stays as it was */
    } finally {
      setBusy(null);
    }
  };
  return (
    <section aria-labelledby="pop-roles-h">
      <h3 id="pop-roles-h" className="lng-pop-h">
        Roles
      </h3>
      <ul className="lng-pop-roles">
        {held.map((r) => (
          <li key={r.id} className="lng-pop-role">
            <span
              className="lng-pop-role-dot"
              aria-hidden="true"
              style={r.color ? { background: r.color } : undefined}
            />
            {r.name}
            {viewer.canManageRole(r.position) && (
              <button
                type="button"
                className="lng-pop-role-x"
                aria-label={`Remove role ${r.name}`}
                disabled={busy === r.id}
                onClick={() => void change(r.id, false)}
              >
                <Icon name="close" size={16} />
              </button>
            )}
          </li>
        ))}
        {held.length === 0 && <li className="lng-muted">No roles</li>}
        {addable.length > 0 && (
          <li>
            <DropdownMenu
              label="Add role"
              items={addable.map((r) => ({
                label: r.name,
                onSelect: () => void change(r.id, true),
              }))}
              trigger={
                <button
                  type="button"
                  className="lng-pop-role lng-pop-role-add"
                  aria-label="Add role"
                >
                  <Icon name="plus" size={16} />
                </button>
              }
            />
          </li>
        )}
      </ul>
    </section>
  );
}

/** Message box under the card: sends a DM without leaving the popout open. */
function MessageBox({ userId, name }: { userId: string; name: string }) {
  const [text, setText] = useState('');
  const [error, setError] = useState(false);
  const openDm = useOpenDm();
  const actions = useLoungeActions();
  const close = usePopout((s) => s.closeProfile);
  const send = async () => {
    const body = text.trim();
    if (!body) return;
    try {
      const dm = await openDm([userId]);
      actions.sendMessage(dm.id, body, null);
      close();
    } catch {
      setError(true);
    }
  };
  return (
    <div>
      <input
        className="lng-pop-input"
        aria-label={`Message @${name}`}
        placeholder={`Message @${name}`}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setError(false);
        }}
        onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            void send();
          }
        }}
      />
      {error && (
        <p role="alert" className="lng-failed">
          Could not send that message.
        </p>
      )}
    </div>
  );
}

function ProfileCard({ target }: { target: ProfileTarget }) {
  const me = useCurrentUser();
  const users = useUsers();
  const user = users[target.userId];
  const member = useMembers(target.guildId).find((m) => m.user.id === target.userId);
  const presence = usePresence(target.userId);
  const state = useProfile(target.userId);
  const openFull = usePopout((s) => s.openFull);
  const profile = state.kind === 'ready' ? state.profile : null;
  const name = member?.nick ?? user?.displayName ?? profile?.user.global_name ?? 'Unknown';
  const handle = user?.username ?? profile?.user.username ?? '';
  const isSelf = target.userId === me.id;
  const avatar = profile ? avatarUrlOf(profile.user) : (user?.avatarUrl ?? null);
  const since = profile?.mutual_guilds?.find((g) => g.id === target.guildId)?.member_since;

  return (
    <>
      <Banner profile={profile} />
      <div className="lng-pop-head">
        <Avatar name={name} src={avatar} size={80} status={presence.status} />
      </div>
      <div className="lng-pop-body">
        <h2 className="lng-pop-name">{name}</h2>
        <p className="lng-pop-handle">
          {handle}
          {profile?.user_profile.pronouns ? ` · ${profile.user_profile.pronouns}` : ''}
        </p>
        {presence.activity && <p className="lng-pop-activity">{presence.activity.text}</p>}
        {state.kind === 'loading' && (
          <p className="lng-muted" role="status">
            Loading profile…
          </p>
        )}
        {profile?.user_profile.bio && (
          <section aria-labelledby="pop-about-h">
            <h3 id="pop-about-h" className="lng-pop-h">
              About me
            </h3>
            <p className="lng-pop-bio">{profile.user_profile.bio}</p>
          </section>
        )}
        {since && (
          <section aria-labelledby="pop-since-h">
            <h3 id="pop-since-h" className="lng-pop-h">
              Member since
            </h3>
            <p>{new Date(since).toLocaleDateString([], { dateStyle: 'medium' })}</p>
          </section>
        )}
        {target.guildId && <RoleChips target={target} />}
        {!isSelf && <NoteField userId={target.userId} />}
        {!isSelf && !user?.bot && <MessageBox userId={target.userId} name={name} />}
        <button
          type="button"
          className="lng-link-btn lng-pop-full"
          onClick={() => openFull({ userId: target.userId, guildId: target.guildId })}
        >
          View Full Profile
        </button>
      </div>
    </>
  );
}

function PopoutFrame({ target, anchor }: { target: ProfileTarget; anchor: HTMLElement }) {
  const close = usePopout((s) => s.closeProfile);
  const portal = usePortalTarget();
  const node = useRef<HTMLDivElement | null>(null);
  const floating = useFloating({
    open: true,
    anchor,
    placement: 'right-start',
    offset: 8,
    padding: 8,
  });

  // Escape or a click elsewhere closes it; focus moves in, and goes back to what opened it.
  useEffect(() => {
    node.current?.focus();
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        close();
      }
    };
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (node.current?.contains(t) || anchor.contains(t)) return;
      // Menus and pickers opened from the popout live in portals of their own.
      if ((t as Element).closest?.('[role="menu"], [role="dialog"], [role="listbox"]')) return;
      close();
    };
    document.addEventListener('keydown', onKey, true);
    document.addEventListener('pointerdown', onDown, true);
    return () => {
      document.removeEventListener('keydown', onKey, true);
      document.removeEventListener('pointerdown', onDown, true);
      if (anchor.isConnected) anchor.focus({ preventScroll: true });
    };
  }, [close, anchor]);

  if (!portal) return null;
  return createPortal(
    <div
      ref={(el) => {
        node.current = el;
        floating.ref(el);
      }}
      className="lng-pop"
      role="dialog"
      aria-label="User profile"
      tabIndex={-1}
      style={floating.style}
    >
      <ProfileCard target={target} />
    </div>,
    portal,
  );
}

/** Mutual servers, friends and connections (View Full Profile). */
function FullProfile({ target, onClose }: { target: ProfileTarget; onClose: () => void }) {
  const users = useUsers();
  const guilds = useGuilds();
  const state = useProfile(target.userId);
  const profile = state.kind === 'ready' ? state.profile : null;
  const user = users[target.userId];
  const name =
    user?.displayName ?? profile?.user.global_name ?? profile?.user.username ?? 'Profile';
  const nameOf = (g: string) => guilds.find((x) => x.id === g)?.name ?? 'A server';
  return (
    <Modal open onClose={onClose} title={name} size="md">
      <Banner profile={profile} />
      <div className="lng-full">
        {state.kind === 'loading' && <p role="status">Loading profile…</p>}
        {state.kind === 'error' && <p role="alert">Could not load this profile.</p>}
        {profile && (
          <>
            <p className="lng-pop-handle">
              {profile.user.username}
              {profile.user_profile.pronouns ? ` · ${profile.user_profile.pronouns}` : ''}
            </p>
            {profile.user_profile.bio && (
              <section>
                <h3 className="lng-pop-h">About me</h3>
                <p className="lng-pop-bio">{profile.user_profile.bio}</p>
              </section>
            )}
            {profile.friends_since && (
              <p>
                Friends since{' '}
                {new Date(profile.friends_since).toLocaleDateString([], { dateStyle: 'medium' })}
              </p>
            )}
            <section>
              <h3 className="lng-pop-h">Mutual servers — {profile.mutual_guilds?.length ?? 0}</h3>
              <ul className="lng-pop-list">
                {(profile.mutual_guilds ?? []).map((g) => (
                  <li key={g.id}>
                    <strong>{nameOf(g.id)}</strong>
                    {g.nick ? <span className="lng-muted"> as {g.nick}</span> : null}
                  </li>
                ))}
              </ul>
            </section>
            <section>
              <h3 className="lng-pop-h">
                Mutual friends —{' '}
                {profile.mutual_friends_count ?? profile.mutual_friends?.length ?? 0}
              </h3>
              <ul className="lng-pop-list">
                {(profile.mutual_friends ?? []).map((f) => (
                  <li key={f.id}>{f.global_name || f.username}</li>
                ))}
              </ul>
            </section>
            {profile.connected_accounts.length > 0 && (
              <section>
                <h3 className="lng-pop-h">Connections</h3>
                <ul className="lng-pop-list">
                  {profile.connected_accounts.map((c) => (
                    <li key={c.id}>
                      {c.name} <span className="lng-muted">({c.type})</span>
                      {c.verified && <Icon name="check" size={16} label="Verified" />}
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

/** Mounted once in the shell: the profile popout and the full profile modal. */
export function PopoutHost() {
  const profile = usePopout((s) => s.profile);
  const full = usePopout((s) => s.full);
  const closeFull = usePopout((s) => s.closeFull);
  return (
    <>
      {profile && (
        <PopoutFrame
          key={`${profile.userId}`}
          target={{ userId: profile.userId, guildId: profile.guildId }}
          anchor={profile.anchor}
        />
      )}
      {full && <FullProfile target={full} onClose={closeFull} />}
    </>
  );
}
