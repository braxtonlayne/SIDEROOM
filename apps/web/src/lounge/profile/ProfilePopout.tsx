import { DropdownMenu, Icon, usePortalTarget, useFloating } from '@sideroom/ui';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { formatDate, useT } from '../../i18n';
import { useViewer } from '../../settings/server/hooks';
import { useCurrentUser, useLoungeActions, useMembers, useRoles, useUsers } from '../data';
import { ProfileBio } from './profileMarkdown';
import { useOpenDm } from '../shell/dm';
import { setMemberRole } from './memberRoles';
import { usePopout, type ProfileTarget } from './popoutStore';
import { Badges } from './Badges';
import { MiniProfile } from '../../profilepage/MiniProfile';
import { FullProfile } from './FullProfile';
import { NoteField, ProfileAvatar, ProfileBanner, StatusBubble, Themed } from './parts';
import { useProfileView } from './profileView';
import './profile.css';
import './profile-p11.css';

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
  const view = useProfileView(target.userId, target.guildId);
  const openFull = usePopout((s) => s.openFull);
  const { profile, presence } = view;
  const isSelf = target.userId === me.id;
  const since = profile?.mutual_guilds?.find((g) => g.id === target.guildId)?.member_since;
  const doing = presence.activities?.[0];

  return (
    <Themed view={view} className="prf-pop-inner">
      <ProfileBanner view={view} />
      <div className="lng-pop-head">
        <ProfileAvatar view={view} />
      </div>
      <div className="lng-pop-body">
        <h2 className="lng-pop-name">{view.name}</h2>
        <p className="lng-pop-handle">
          {view.handle}
          {view.pronouns ? ` • ${view.pronouns}` : ''}
        </p>
        <Badges badges={view.badges} />
        <StatusBubble activity={presence.custom} />
        {doing && <p className="lng-pop-activity">{doing.text}</p>}
        {view.loading && (
          <p className="lng-muted" role="status">
            {t('lounge.profilePopout.loading')}
          </p>
        )}
        {view.bio && (
          <section aria-labelledby="pop-about-h">
            <h3 id="pop-about-h" className="lng-pop-h">
              {t('lounge.profilePopout.aboutMe')}
            </h3>
            <ProfileBio text={view.bio} />
          </section>
        )}
        {!user?.bot && <MiniProfile userId={target.userId} />}
        {since && (
          <section aria-labelledby="pop-since-h">
            <h3 id="pop-since-h" className="lng-pop-h">
              {t('lounge.profilePopout.memberSince')}
            </h3>
            <p>{formatDate(new Date(since), { dateStyle: 'medium' })}</p>
          </section>
        )}
        {target.guildId && <RoleChips target={target} />}
        {!isSelf && <NoteField userId={target.userId} />}
        {!isSelf && !user?.bot && <MessageBox userId={target.userId} name={view.name} />}
        <button
          type="button"
          className="lng-link-btn lng-pop-full"
          onClick={() => openFull({ userId: target.userId, guildId: target.guildId })}
        >
          {t('lounge.profilePopout.viewFull')}
        </button>
      </div>
    </Themed>
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
