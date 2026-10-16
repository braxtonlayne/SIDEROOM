import { Avatar, Button, Notice, Switch, useToast } from '@sideroom/ui';
import { useState } from 'react';
import {
  errorText,
  getPrivacy,
  listIgnores,
  patchPrivacy,
  listRelationships,
  unblockUser,
  unignoreUser,
  useResource,
  type PrivacySettings,
  type UserLite,
} from '../api';
import {
  setLoadRemoteImages,
  useEnsureUserSettings,
  useLoadRemoteImages,
} from '../../../lounge/msg/remoteImages';
import { useT, type MessageKey } from '../../../i18n';
import { DataExportCard } from './DataExport';
import { Card, displayOf, Loaded, SectionTitle, Unavailable } from '../parts';

function UserList({
  users,
  empty,
  action,
  onAction,
}: {
  users: UserLite[];
  empty: string;
  action: string;
  onAction: (u: UserLite) => Promise<void>;
}) {
  const t = useT();
  const [error, setError] = useState<string | null>(null);
  if (users.length === 0) return <p className="us-muted">{empty}</p>;
  return (
    <>
      <ul className="us-list">
        {users.map((u) => (
          <li key={u.id}>
            <div className="us-row-main">
              <Avatar name={displayOf(u)} size={32} /> <strong>{displayOf(u)}</strong>
              {u.username && u.global_name && <span className="us-muted"> {u.username}</span>}
            </div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setError(null);
                onAction(u).catch((e: unknown) => setError(errorText(e)));
              }}
            >
              {action}
              <span className="us-sr"> {displayOf(u)}</span>
            </Button>
          </li>
        ))}
      </ul>
      {error && (
        <Notice variant="danger" title={t('settings.privacy.userListError')}>
          {error}
        </Notice>
      )}
    </>
  );
}

function BlockedCard() {
  const t = useT();
  const rels = useResource(listRelationships);
  return (
    <Card id="us-blocked" title={t('settings.privacy.blockedTitle')}>
      <Loaded res={rels} what={t('settings.privacy.blockedWhat')}>
        {(all) => (
          <UserList
            users={all.filter((r) => r.type === 2).map((r) => r.user)}
            empty={t('settings.privacy.blockedEmpty')}
            action={t('settings.privacy.unblock')}
            onAction={async (u) => {
              await unblockUser(u.id);
              rels.setData((d) => d?.filter((r) => !(r.type === 2 && r.user.id === u.id)));
            }}
          />
        )}
      </Loaded>
    </Card>
  );
}

function IgnoredCard() {
  const t = useT();
  const ignores = useResource(listIgnores);
  return (
    <Card
      id="us-ignored"
      title={t('settings.privacy.ignoredTitle')}
      description={t('settings.privacy.ignoredDesc')}
    >
      <Loaded res={ignores} what={t('settings.privacy.ignoredWhat')}>
        {(all) => (
          <UserList
            users={all}
            empty={t('settings.privacy.ignoredEmpty')}
            action={t('settings.privacy.unignore')}
            onAction={async (u) => {
              await unignoreUser(u.id);
              ignores.setData((d) => d?.filter((x) => x.id !== u.id));
            }}
          />
        )}
      </Loaded>
    </Card>
  );
}

export function RemoteImagesCard() {
  const t = useT();
  useEnsureUserSettings();
  const on = useLoadRemoteImages();
  const [error, setError] = useState<string | null>(null);
  return (
    <Card
      id="us-remote-images"
      title={t('settings.privacy.remoteTitle')}
      description={t('settings.privacy.remoteDesc')}
    >
      <Switch
        label={t('settings.privacy.remoteLoad')}
        description={t('settings.privacy.remoteLoadDesc')}
        checked={on}
        onChange={(e) => {
          setError(null);
          setLoadRemoteImages(e.target.checked).catch((err: unknown) => setError(errorText(err)));
        }}
      />
      {error && (
        <Notice variant="danger" title={t('settings.privacy.saveError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

const FRIEND_SOURCES: { key: keyof PrivacySettings; label: MessageKey; hint: MessageKey }[] = [
  {
    key: 'friend_requests_everyone',
    label: 'settings.privacy.friendEveryone',
    hint: 'settings.privacy.friendEveryoneHint',
  },
  {
    key: 'friend_requests_friends_of_friends',
    label: 'settings.privacy.friendFof',
    hint: 'settings.privacy.friendFofHint',
  },
  {
    key: 'friend_requests_server_members',
    label: 'settings.privacy.friendServer',
    hint: 'settings.privacy.friendServerHint',
  },
];

/** Friend-request sources, server-member DMs and Message Requests (`/users/@me/privacy`), saved on toggle. */
function ReachCard() {
  const t = useT();
  const toast = useToast();
  const res = useResource(getPrivacy);
  const [error, setError] = useState<string | null>(null);
  const set = async (patch: Partial<PrivacySettings>) => {
    setError(null);
    const before = res.data;
    res.setData((d) => (d ? { ...d, ...patch } : d));
    try {
      res.setData(await patchPrivacy(patch));
      toast.show({ title: t('settings.privacy.updated'), duration: 1500 });
    } catch (e) {
      if (before) res.setData(before);
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-who"
      title={t('settings.privacy.reachTitle')}
      description={t('settings.privacy.reachDesc')}
    >
      <Loaded res={res} what={t('settings.privacy.reachWhat')}>
        {(p) => (
          <div className="us-stack">
            <div role="group" aria-labelledby="us-fr-h" className="us-stack">
              <h3 id="us-fr-h" className="us-subhead">
                {t('settings.privacy.friendRequests')}
              </h3>
              {FRIEND_SOURCES.map((s) => (
                <Switch
                  key={s.key}
                  label={t(s.label)}
                  description={t(s.hint)}
                  checked={p[s.key]}
                  onChange={(e) => void set({ [s.key]: e.target.checked })}
                />
              ))}
            </div>
            <Switch
              label={t('settings.privacy.serverDms')}
              description={t('settings.privacy.serverDmsDesc')}
              checked={p.allow_server_member_dms}
              onChange={(e) => void set({ allow_server_member_dms: e.target.checked })}
            />
            <Switch
              label={t('settings.privacy.requests')}
              description={t('settings.privacy.requestsDesc')}
              checked={p.message_requests}
              onChange={(e) => void set({ message_requests: e.target.checked })}
            />
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.privacy.saveError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

export function PrivacySection() {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.privacy.title')}</SectionTitle>
      <ReachCard />
      <Card
        id="us-filters"
        title={t('settings.privacy.sensitiveTitle')}
        description={t('settings.privacy.sensitiveDesc')}
      >
        <Unavailable>{t('settings.privacy.sensitiveUnavailable')}</Unavailable>
      </Card>
      <RemoteImagesCard />
      <BlockedCard />
      <IgnoredCard />
      <DataExportCard />
    </>
  );
}
