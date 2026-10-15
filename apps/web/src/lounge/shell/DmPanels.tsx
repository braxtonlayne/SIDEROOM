import { Avatar, Button, Icon, Modal, TextInput } from '@sideroom/ui';
import { useRef, useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { formatDate, useT } from '../../i18n';
import { readAsDataUri } from '../../settings/user/imageCrop';
import { rest, useStore } from '../../store';
import { dmTitle } from '../channels';
import { useCurrentUser, useDms, usePresence, useUsers, type DmChannel } from '../data';
import { inviteUrl } from '../inviteFlow';
import { snowflakeTime } from '../liveMap';
import { Md } from '../msg/Md';
import {
  FriendAction,
  ProfileAvatar,
  ProfileBanner,
  ProfileMoreMenu,
  StatusBubble,
  Themed,
} from '../profile/parts';
import { openProfilePopout, usePopout } from '../profile/popoutStore';
import { Badges } from '../profile/Badges';
import { useProfileView } from '../profile/profileView';
import { useLoungeUi } from '../store';

/** Right-hand panel of a one-to-one DM (LNG-DM-006): the other person's profile at a glance. */
function UserProfilePanel({ userId }: { userId: string }) {
  const t = useT();
  const view = useProfileView(userId);
  const openFull = usePopout((s) => s.openFull);
  const users = useUsers();
  const { profile, presence } = view;
  const since = profile?.created_at ?? snowflakeTime(userId);
  const servers = profile?.mutual_guilds?.length ?? 0;
  const friends = profile?.mutual_friends_count ?? profile?.mutual_friends?.length ?? 0;
  const isBot = !!users[userId]?.bot || !!profile?.user.bot;
  const doing = presence.activities?.[0];
  return (
    <div className="lng-profile" aria-label={t('lounge.panels.profileLabel', { name: view.name })}>
      <Themed view={view} className="prf-dm-panel">
        <ProfileBanner view={view} />
        <div className="lng-profile-avatar">
          <ProfileAvatar view={view} />
        </div>
        <div className="lng-profile-card">
          <h2>{view.name}</h2>
          <p className="lng-muted">
            {view.handle}
            {view.pronouns ? ` • ${view.pronouns}` : ''}
          </p>
          <Badges badges={view.badges} />
          <div className="prf-dm-actions">
            <FriendAction userId={userId} isBot={isBot} />
            <ProfileMoreMenu
              userId={userId}
              name={view.name}
              username={view.handle}
              isSelf={false}
            />
          </div>
          <StatusBubble activity={presence.custom} />
          {doing && <p>{doing.text}</p>}
          {view.bio && (
            <section>
              <h3 className="lng-pop-h">{t('lounge.profilePopout.aboutMe')}</h3>
              <div className="lng-pop-bio">
                <Md text={view.bio} />
              </div>
            </section>
          )}
          {since && (
            <section>
              <h3 className="lng-pop-h">{t('lounge.profilePopout.memberSince')}</h3>
              <p>{formatDate(new Date(since), { dateStyle: 'medium' })}</p>
            </section>
          )}
          {profile && (
            <button
              type="button"
              className="lng-link-btn prf-dm-mutual"
              onClick={() => openFull({ userId })}
            >
              {t('lounge.dmPanel.mutualSummary', { servers, friends })}
            </button>
          )}
          <button type="button" className="lng-link-btn" onClick={() => openFull({ userId })}>
            {t('lounge.profilePopout.viewFull')}
          </button>
        </div>
      </Themed>
    </div>
  );
}

/** Group name and icon (`PATCH /channels/{id}`), open to every member like on Discord. */
function GroupSettingsDialog({ dm, onClose }: { dm: DmChannel; onClose: () => void }) {
  const t = useT();
  const [name, setName] = useState(dm.name ?? '');
  const [icon, setIcon] = useState<string | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const file = useRef<HTMLInputElement>(null);
  const shown = icon === undefined ? (dm.iconUrl ?? null) : icon;

  const save = async () => {
    setBusy(true);
    setError(null);
    const body: Record<string, string | null> = { name: name.trim() || null };
    if (icon !== undefined) body.icon = icon;
    try {
      const ch = await apiFetch<{ id: string; name?: string | null; icon?: string | null }>(
        `/channels/${encodeURIComponent(dm.id)}`,
        { method: 'PATCH', json: body, retries: 0 },
      );
      useStore.getState().applyDispatch('CHANNEL_UPDATE', {
        ...ch,
        id: dm.id,
        type: 3,
        recipients: useStore.getState().channels[dm.id]?.recipients ?? [],
      });
      onClose();
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.dmPanel.saveError'));
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.dmPanel.settingsTitle')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.dmPanel.cancel')}
          </Button>
          <Button loading={busy} onClick={() => void save()}>
            {t('lounge.dmPanel.save')}
          </Button>
        </>
      }
    >
      <div className="lng-form">
        <TextInput
          label={t('lounge.dmPanel.groupName')}
          value={name}
          maxLength={100}
          onChange={(e) => setName(e.target.value)}
        />
        <div className="lng-gp-image-row">
          <Avatar name={name || t('lounge.dmPanel.groupIcon')} src={shown} size={56} />
          <input
            ref={file}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            hidden
            aria-label={t('lounge.dmPanel.iconFile')}
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = '';
              if (f) readAsDataUri(f).then(setIcon, () => setError(t('lounge.dmPanel.readError')));
            }}
          />
          <Button size="sm" variant="secondary" onClick={() => file.current?.click()}>
            {t('lounge.dmPanel.changeIcon')}
          </Button>
          {shown && (
            <Button size="sm" variant="ghost" onClick={() => setIcon(null)}>
              {t('lounge.dmPanel.removeIcon')}
            </Button>
          )}
        </div>
        {error && (
          <p role="alert" className="lng-failed">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** Owner-only invite link for the group (contract: `POST /channels/{id}/invites` on a group DM). */
function GroupInvite({ channelId }: { channelId: string }) {
  const t = useT();
  const [state, setState] = useState<
    { kind: 'idle' } | { kind: 'busy' } | { kind: 'ready'; url: string } | { kind: 'error' }
  >({ kind: 'idle' });
  const [copied, setCopied] = useState(false);
  const create = () => {
    setState({ kind: 'busy' });
    rest.createInvite(channelId).then(
      (inv) => setState({ kind: 'ready', url: inviteUrl(inv.code) }),
      () => setState({ kind: 'error' }),
    );
  };
  return (
    <section aria-label={t('lounge.dmPanel.inviteHeading')}>
      <h3 className="lng-pop-h">{t('lounge.dmPanel.inviteHeading')}</h3>
      {state.kind === 'ready' ? (
        <div className="prf-invite">
          <input
            readOnly
            className="lng-pop-input"
            aria-label={t('lounge.dmPanel.inviteLink')}
            value={state.url}
            onFocus={(e) => e.currentTarget.select()}
          />
          <Button
            size="sm"
            variant="secondary"
            onClick={() => {
              void navigator.clipboard?.writeText(state.url);
              setCopied(true);
            }}
          >
            {copied ? t('lounge.dmPanel.copied') : t('lounge.dmPanel.copy')}
          </Button>
        </div>
      ) : (
        <Button size="sm" variant="secondary" loading={state.kind === 'busy'} onClick={create}>
          {t('lounge.dmPanel.createInvite')}
        </Button>
      )}
      {state.kind === 'error' && (
        <p role="alert" className="lng-failed">
          {t('lounge.dmPanel.inviteError')}
        </p>
      )}
    </section>
  );
}

function MemberRow({ dm, userId, isOwner }: { dm: DmChannel; userId: string; isOwner: boolean }) {
  const t = useT();
  const users = useUsers();
  const me = useCurrentUser();
  const presence = usePresence(userId);
  const [error, setError] = useState(false);
  const name = users[userId]?.displayName ?? t('lounge.homeSidebar.unknown');
  const viewerIsOwner = dm.ownerId === me.id;
  return (
    <li className="prf-member">
      <button
        type="button"
        className="prf-member-main"
        aria-label={t('lounge.dmPanel.openProfile', { name })}
        onClick={(e) => openProfilePopout({ userId }, e.currentTarget)}
      >
        <Avatar
          name={name}
          src={users[userId]?.avatarUrl ?? null}
          size={32}
          status={presence.status}
        />
        <span className="prf-member-name">{name}</span>
        {isOwner && (
          <span className="lng-tag" title={t('lounge.dmPanel.ownerHint')}>
            <Icon name="star" size={16} label={t('lounge.dmPanel.owner')} />
          </span>
        )}
      </button>
      {viewerIsOwner && userId !== me.id && (
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.dmPanel.remove', { name })}
          onClick={() => {
            setError(false);
            apiFetch<void>(
              `/channels/${encodeURIComponent(dm.id)}/recipients/${encodeURIComponent(userId)}`,
              { method: 'DELETE', retries: 0 },
            ).catch(() => setError(true));
          }}
        >
          <Icon name="close" size={16} />
        </button>
      )}
      {error && (
        <span role="alert" className="lng-failed">
          {t('lounge.dmPanel.removeError')}
        </span>
      )}
    </li>
  );
}

/** Group DM panel (LNG-DM-007): members, invite link for the owner, group settings. */
function GroupPanel({ dm }: { dm: DmChannel }) {
  const t = useT();
  const me = useCurrentUser();
  const users = useUsers();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const [settings, setSettings] = useState(false);
  const title = dmTitle(dm, (id) => users[id]?.displayName ?? t('lounge.homeSidebar.unknown'));
  const everyone = [me.id, ...dm.recipientIds];
  return (
    <div className="lng-profile" aria-label={t('lounge.panels.groupMembers')}>
      <div className="prf-group-head">
        <Avatar name={title} src={dm.iconUrl ?? null} size={56} />
        <h2 className="lng-profile-h">
          {t('lounge.panels.membersCount', { count: everyone.length })}
        </h2>
      </div>
      <ul className="lng-profile-members prf-members">
        {everyone.map((id) => (
          <MemberRow key={id} dm={dm} userId={id} isOwner={dm.ownerId === id} />
        ))}
      </ul>
      <div className="prf-group-actions">
        <Button
          size="sm"
          variant="secondary"
          onClick={() => openDialog({ kind: 'addToDm', channelId: dm.id })}
        >
          {t('lounge.dmPanel.addFriends')}
        </Button>
        <Button size="sm" variant="secondary" onClick={() => setSettings(true)}>
          {t('lounge.dmPanel.settings')}
        </Button>
      </div>
      {dm.ownerId === me.id && <GroupInvite channelId={dm.id} />}
      {settings && <GroupSettingsDialog dm={dm} onClose={() => setSettings(false)} />}
    </div>
  );
}

/** The right panel of a DM: a profile for one person, the member list for a group. */
export function DmSidePanel({ dmId }: { dmId: string }) {
  const dm = useDms().find((d) => d.id === dmId);
  if (!dm || dm.recipientIds.length === 0) return null;
  if (dm.recipientIds.length > 1) return <GroupPanel dm={dm} />;
  return <UserProfilePanel userId={dm.recipientIds[0]!} />;
}
