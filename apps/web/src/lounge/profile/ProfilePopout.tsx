import { Avatar, DropdownMenu, Icon, Modal, usePortalTarget, useFloating } from '@sideroom/ui';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
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
  const t = useT();
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
        {t('lounge.profilePopout.note')}
      </h3>
      <input
        className="lng-pop-input"
        aria-label={t('lounge.profilePopout.noteLabel')}
        placeholder={t('lounge.profilePopout.notePlaceholder')}
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
  const t = useT();
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
        {t('lounge.profilePopout.roles')}
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
                aria-label={t('lounge.profilePopout.removeRole', { name: r.name })}
                disabled={busy === r.id}
                onClick={() => void change(r.id, false)}
              >
                <Icon name="close" size={16} />
              </button>
            )}
          </li>
        ))}
        {held.length === 0 && <li className="lng-muted">{t('lounge.profilePopout.noRoles')}</li>}
        {addable.length > 0 && (
          <li>
            <DropdownMenu
              label={t('lounge.profilePopout.addRole')}
              items={addable.map((r) => ({
                label: r.name,
                onSelect: () => void change(r.id, true),
              }))}
              trigger={
                <button
                  type="button"
                  className="lng-pop-role lng-pop-role-add"
                  aria-label={t('lounge.profilePopout.addRole')}
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
  const t = useT();
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
        aria-label={t('lounge.profilePopout.messageLabel', { name })}
        placeholder={t('lounge.profilePopout.messageLabel', { name })}
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
          {t('lounge.profilePopout.sendError')}
        </p>
      )}
    </div>
  );
}

function ProfileCard({ target }: { target: ProfileTarget }) {
  const t = useT();
  const me = useCurrentUser();
  const users = useUsers();
  const user = users[target.userId];
  const member = useMembers(target.guildId).find((m) => m.user.id === target.userId);
  const presence = usePresence(target.userId);
  const state = useProfile(target.userId);
  const openFull = usePopout((s) => s.openFull);
  const profile = state.kind === 'ready' ? state.profile : null;
  const name =
    member?.nick ??
    user?.displayName ??
    profile?.user.global_name ??
    t('lounge.profilePopout.unknown');
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
            {t('lounge.profilePopout.loading')}
          </p>
        )}
        {profile?.user_profile.bio && (
          <section aria-labelledby="pop-about-h">
            <h3 id="pop-about-h" className="lng-pop-h">
              {t('lounge.profilePopout.aboutMe')}
            </h3>
            <p className="lng-pop-bio">{profile.user_profile.bio}</p>
          </section>
        )}
        {since && (
          <section aria-labelledby="pop-since-h">
            <h3 id="pop-since-h" className="lng-pop-h">
              {t('lounge.profilePopout.memberSince')}
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
          {t('lounge.profilePopout.viewFull')}
        </button>
      </div>
    </>
  );
}

function PopoutFrame({ target, anchor }: { target: ProfileTarget; anchor: HTMLElement }) {
  const t = useT();
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
      const tgt = e.target as Node;
      if (node.current?.contains(tgt) || anchor.contains(tgt)) return;
      // Menus and pickers opened from the popout live in portals of their own.
      if ((tgt as Element).closest?.('[role="menu"], [role="dialog"], [role="listbox"]')) return;
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
      aria-label={t('lounge.profilePopout.userProfile')}
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
  const t = useT();
  const users = useUsers();
  const guilds = useGuilds();
  const state = useProfile(target.userId);
  const profile = state.kind === 'ready' ? state.profile : null;
  const user = users[target.userId];
  const name =
    user?.displayName ??
    profile?.user.global_name ??
    profile?.user.username ??
    t('lounge.profilePopout.profileFallback');
  const nameOf = (g: string) =>
    guilds.find((x) => x.id === g)?.name ?? t('lounge.profilePopout.aServer');
  return (
    <Modal open onClose={onClose} title={name} size="md">
      <Banner profile={profile} />
      <div className="lng-full">
        {state.kind === 'loading' && <p role="status">{t('lounge.profilePopout.loading')}</p>}
        {state.kind === 'error' && <p role="alert">{t('lounge.profilePopout.loadError')}</p>}
        {profile && (
          <>
            <p className="lng-pop-handle">
              {profile.user.username}
              {profile.user_profile.pronouns ? ` · ${profile.user_profile.pronouns}` : ''}
            </p>
            {profile.user_profile.bio && (
              <section>
                <h3 className="lng-pop-h">{t('lounge.profilePopout.aboutMe')}</h3>
                <p className="lng-pop-bio">{profile.user_profile.bio}</p>
              </section>
            )}
            {profile.friends_since && (
              <p>
                {t('lounge.profilePopout.friendsSince', {
                  date: new Date(profile.friends_since).toLocaleDateString([], {
                    dateStyle: 'medium',
                  }),
                })}
              </p>
            )}
            <section>
              <h3 className="lng-pop-h">
                {t('lounge.profilePopout.mutualServers', {
                  count: profile.mutual_guilds?.length ?? 0,
                })}
              </h3>
              <ul className="lng-pop-list">
                {(profile.mutual_guilds ?? []).map((g) => (
                  <li key={g.id}>
                    <strong>{nameOf(g.id)}</strong>
                    {g.nick ? (
                      <span className="lng-muted">
                        {t('lounge.profilePopout.asNick', { nick: g.nick })}
                      </span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </section>
            <section>
              <h3 className="lng-pop-h">
                {t('lounge.profilePopout.mutualFriends', {
                  count: profile.mutual_friends_count ?? profile.mutual_friends?.length ?? 0,
                })}
              </h3>
              <ul className="lng-pop-list">
                {(profile.mutual_friends ?? []).map((f) => (
                  <li key={f.id}>{f.global_name || f.username}</li>
                ))}
              </ul>
            </section>
            {profile.connected_accounts.length > 0 && (
              <section>
                <h3 className="lng-pop-h">{t('lounge.profilePopout.connections')}</h3>
                <ul className="lng-pop-list">
                  {profile.connected_accounts.map((c) => (
                    <li key={c.id}>
                      {c.name} <span className="lng-muted">({c.type})</span>
                      {c.verified && (
                        <Icon name="check" size={16} label={t('lounge.profilePopout.verified')} />
                      )}
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
