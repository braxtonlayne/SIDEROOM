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
        <Notice variant="danger" title="That didn't work">
          {error}
        </Notice>
      )}
    </>
  );
}

function BlockedCard() {
  const rels = useResource(listRelationships);
  return (
    <Card id="us-blocked" title="Blocked users">
      <Loaded res={rels} what="blocked users">
        {(all) => (
          <UserList
            users={all.filter((r) => r.type === 2).map((r) => r.user)}
            empty="You haven't blocked anyone."
            action="Unblock"
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
  const ignores = useResource(listIgnores);
  return (
    <Card
      id="us-ignored"
      title="Ignored users"
      description="Their messages and DM notifications are hidden; the friendship stays."
    >
      <Loaded res={ignores} what="ignored users">
        {(all) => (
          <UserList
            users={all}
            empty="You haven't ignored anyone."
            action="Unignore"
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
  useEnsureUserSettings();
  const on = useLoadRemoteImages();
  const [error, setError] = useState<string | null>(null);
  return (
    <Card
      id="us-remote-images"
      title="Embeds and images"
      description="Images in link previews are fetched through this server, which keeps your address private."
    >
      <Switch
        label="Load remote images automatically"
        description="Show images that are still hosted on other sites without asking first. Those sites can see your IP address."
        checked={on}
        onChange={(e) => {
          setError(null);
          setLoadRemoteImages(e.target.checked).catch((err: unknown) => setError(errorText(err)));
        }}
      />
      {error && (
        <Notice variant="danger" title="Couldn't save that">
          {error}
        </Notice>
      )}
    </Card>
  );
}

const FRIEND_SOURCES: { key: keyof PrivacySettings; label: string; hint: string }[] = [
  { key: 'friend_requests_everyone', label: 'Everyone', hint: 'Anyone who finds your username.' },
  {
    key: 'friend_requests_friends_of_friends',
    label: 'Friends of friends',
    hint: 'People you have a friend in common with.',
  },
  {
    key: 'friend_requests_server_members',
    label: 'Server members',
    hint: 'People who share a server with you.',
  },
];

/** Friend-request sources, server-member DMs and Message Requests (`/users/@me/privacy`), saved on toggle. */
function ReachCard() {
  const toast = useToast();
  const res = useResource(getPrivacy);
  const [error, setError] = useState<string | null>(null);
  const set = async (patch: Partial<PrivacySettings>) => {
    setError(null);
    const before = res.data;
    res.setData((d) => (d ? { ...d, ...patch } : d));
    try {
      res.setData(await patchPrivacy(patch));
      toast.show({ title: 'Privacy updated', duration: 1500 });
    } catch (e) {
      if (before) res.setData(before);
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-who"
      title="Who can reach you"
      description="These save as soon as you change them."
    >
      <Loaded res={res} what="your privacy settings">
        {(p) => (
          <div className="us-stack">
            <div role="group" aria-labelledby="us-fr-h" className="us-stack">
              <h3 id="us-fr-h" className="us-subhead">
                Who can send you friend requests
              </h3>
              {FRIEND_SOURCES.map((s) => (
                <Switch
                  key={s.key}
                  label={s.label}
                  description={s.hint}
                  checked={p[s.key]}
                  onChange={(e) => void set({ [s.key]: e.target.checked })}
                />
              ))}
            </div>
            <Switch
              label="Allow direct messages from server members"
              description="People who share a server with you can message you. Turn it off and only friends can."
              checked={p.allow_server_member_dms}
              onChange={(e) => void set({ allow_server_member_dms: e.target.checked })}
            />
            <Switch
              label="Message requests"
              description="Messages from people who aren't your friends wait in Message Requests until you accept."
              checked={p.message_requests}
              onChange={(e) => void set({ message_requests: e.target.checked })}
            />
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title="Couldn't save that">
          {error}
        </Notice>
      )}
    </Card>
  );
}

export function PrivacySection() {
  return (
    <>
      <SectionTitle>Privacy &amp; Safety</SectionTitle>
      <ReachCard />
      <Card
        id="us-filters"
        title="Sensitive media and age-restricted content"
        description="Blur or block sensitive images from friends, others and servers, and re-confirm your age."
      >
        <Unavailable>
          The server has no media-scanning or per-source filter settings yet, and no way to read or
          reset your age confirmation. Age-restricted channels and library titles still ask you to
          confirm when you open them.
        </Unavailable>
      </Card>
      <RemoteImagesCard />
      <BlockedCard />
      <IgnoredCard />
      <Card
        id="us-data"
        title="Request my data"
        description="Get a copy of your messages, profile and uploads."
      >
        <Button variant="secondary" disabled>
          Request my data
        </Button>
        <Unavailable>
          The data export package (JSON plus an attachment index, with a 7-day download link)
          isn&apos;t built on the server yet, so there is nothing to request.
        </Unavailable>
      </Card>
    </>
  );
}
