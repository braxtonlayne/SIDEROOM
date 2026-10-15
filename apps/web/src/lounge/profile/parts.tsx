/** Pieces shared by the profile popout, the full profile and the DM profile panel. */
import { Avatar, Button, DropdownMenu, Icon, Tooltip, type MenuEntry } from '@sideroom/ui';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { apiFetch } from '../../api/client';
import { useDeveloperMode } from '../../settings/user/prefs';
import { useT } from '../../i18n';
import type { Activity } from '../data';
import { acceptFriend } from '../shell/api';
import { friendsOf, useSocial } from '../shell/social';
import { useUserMenuEntries } from '../shell/userMenu';
import { REL } from '../shell/api';
import { decorationNode, ProfileEffect } from './decoration';
import type { ProfileView } from './profileView';

const NOTE_MAX = 256;

/** The banner strip: the image, else the accent or first theme color. */
export function ProfileBanner({ view, tall }: { view: ProfileView; tall?: boolean }) {
  return (
    <div
      className="lng-pop-banner prf-banner"
      data-tall={tall || undefined}
      style={{
        ...(view.accent ? { background: view.accent } : {}),
        ...(view.bannerUrl ? { backgroundImage: `url(${view.bannerUrl})` } : {}),
      }}
    />
  );
}

/** Avatar with status, decoration and the optional effect layer behind it. */
export function ProfileAvatar({ view, size = 80 }: { view: ProfileView; size?: 56 | 80 }) {
  return (
    <Avatar
      name={view.name}
      src={view.avatarUrl}
      size={size}
      status={view.presence.status}
      decoration={decorationNode(view.decoration)}
    />
  );
}

/** Applies the theme (gradient + text colors) and the effect layer to a profile surface. */
export function Themed({
  view,
  className,
  children,
}: {
  view: ProfileView;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={`prf-themed${className ? ` ${className}` : ''}`}
      data-themed={view.theme ? view.theme.tone : undefined}
      style={view.theme?.style}
    >
      <ProfileEffect effect={view.effect} />
      {children}
    </div>
  );
}

/** Private note about the person, saved when the box loses focus (`GET/PUT /users/@me/notes/{id}`). */
export function NoteField({ userId }: { userId: string }) {
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
    <section aria-labelledby={`note-h-${userId}`}>
      <h3 id={`note-h-${userId}`} className="lng-pop-h">
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

/** Add Friend, or a "Friends" / "Request sent" mark, from the relationship list. */
export function FriendAction({ userId, isBot }: { userId: string; isBot?: boolean | undefined }) {
  const t = useT();
  const rows = useSocial((s) => s.relationships);
  const status = useSocial((s) => s.status);
  const load = useSocial((s) => s.loadRelationships);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (status === 'idle') void load();
  }, [status, load]);
  if (isBot) return null;
  const row = rows.find((r) => r.id === userId);
  if (row?.type === REL.BLOCKED) return null;
  if (friendsOf(rows).some((r) => r.id === userId))
    return (
      <Tooltip content={t('lounge.profileActions.friends')}>
        <span
          className="prf-friend-mark"
          role="img"
          aria-label={t('lounge.profileActions.friends')}
        >
          <Icon name="check" size={20} />
        </span>
      </Tooltip>
    );
  if (row?.type === REL.OUTGOING)
    return (
      <Button size="sm" variant="secondary" disabled>
        {t('lounge.profileActions.requestSent')}
      </Button>
    );
  const label =
    row?.type === REL.INCOMING
      ? t('lounge.profileActions.acceptRequest')
      : t('lounge.profileActions.addFriend');
  return (
    <>
      <Button
        size="sm"
        variant="secondary"
        loading={busy}
        onClick={() => {
          setBusy(true);
          setFailed(false);
          acceptFriend(userId)
            // The gateway (RELATIONSHIP_*) updates the list; reload covers a quiet socket.
            .then(() => load())
            .catch(() => setFailed(true))
            .finally(() => setBusy(false));
        }}
      >
        {label}
      </Button>
      {failed && (
        <span role="alert" className="lng-failed">
          {t('lounge.profileActions.friendError')}
        </span>
      )}
    </>
  );
}

/**
 * The ⋯ menu of a profile (LNG-PRF-007): Copy Username, Invite to Server, Add Note, Remove Friend, Ignore, Block,
 * Report, Copy User ID. The relationship entries come from the same helper the DM list and member list use.
 */
export function ProfileMoreMenu({
  userId,
  name,
  username,
  isSelf,
}: {
  userId: string;
  name: string;
  username: string;
  isSelf: boolean;
}) {
  const t = useT();
  const developerMode = useDeveloperMode();
  const entries = useUserMenuEntries(isSelf ? undefined : { id: userId, name });
  const copy = (text: string) => void navigator.clipboard?.writeText(text);
  const items: MenuEntry[] = [
    { label: t('lounge.profileActions.copyUsername'), onSelect: () => copy(username) },
    ...(entries.invite ? [entries.invite] : []),
    ...(entries.note ? [entries.note] : []),
    ...(entries.relationship.length ? [{ type: 'separator' } as MenuEntry] : []),
    ...entries.relationship,
    ...(developerMode
      ? [
          { type: 'separator' } as MenuEntry,
          { label: t('lounge.profileActions.copyUserId'), onSelect: () => copy(userId) },
        ]
      : []),
  ];
  return (
    <DropdownMenu
      label={t('lounge.profileActions.moreFor', { name })}
      items={items}
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.profileActions.moreFor', { name })}
        >
          <Icon name="more-horizontal" size={20} />
        </button>
      }
    />
  );
}

const KIND_ICON = {
  playing: 'play',
  watching: 'tv',
  reading: 'book',
  listening: 'volume',
  custom: 'info',
} as const;

/** The custom-status bubble (speech-bubble style) above the profile body. */
export function StatusBubble({ activity }: { activity: Activity | undefined }) {
  if (!activity) return null;
  return <p className="prf-bubble">{activity.text}</p>;
}

/** Activity cards of one person: game, watching, reading, listening. */
export function ActivityList({ activities }: { activities: Activity[] }) {
  const t = useT();
  return (
    <ul className="prf-activities">
      {activities.map((a, i) => (
        <li key={`${a.kind}-${a.text}-${i}`} className="prf-activity" data-kind={a.kind}>
          <Icon name={KIND_ICON[a.kind]} size={20} />
          <span className="prf-activity-text">
            <strong>
              {
                {
                  playing: t('lounge.friends.activityPlaying', { text: a.text }),
                  watching: t('lounge.friends.activityWatching', { text: a.text }),
                  reading: t('lounge.friends.activityReading', { text: a.text }),
                  listening: t('lounge.friends.activityListening', { text: a.text }),
                  custom: a.text,
                }[a.kind]
              }
            </strong>
            {a.details && <span className="lng-muted">{a.details}</span>}
          </span>
        </li>
      ))}
    </ul>
  );
}
