import { Avatar, Button, Notice, Select, Switch } from '@sideroom/ui';
import { useState } from 'react';
import {
  errorText,
  listIgnores,
  listRelationships,
  unblockUser,
  unignoreUser,
  useResource,
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

function RemoteImagesCard() {
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

const WHO = [
  { value: 'everyone', label: 'Everyone' },
  { value: 'friends', label: 'Friends only' },
  { value: 'none', label: 'No one' },
];

export function PrivacySection() {
  return (
    <>
      <SectionTitle>Privacy &amp; Safety</SectionTitle>
      <Card id="us-who" title="Who can reach you">
        <div className="us-stack">
          <label className="us-muted" id="us-dm-label">
            Who can send you direct messages
          </label>
          <Select aria-labelledby="us-dm-label" options={WHO} value="everyone" disabled />
          <label className="us-muted" id="us-fr-label">
            Who can send you friend requests
          </label>
          <Select aria-labelledby="us-fr-label" options={WHO} value="everyone" disabled />
        </div>
        <Unavailable>
          The server has no endpoint for DM and friend-request privacy yet, so these can&apos;t be
          changed.
        </Unavailable>
      </Card>
      <RemoteImagesCard />
      <BlockedCard />
      <IgnoredCard />
    </>
  );
}
