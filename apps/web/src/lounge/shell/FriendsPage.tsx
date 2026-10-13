import {
  Avatar,
  Badge,
  Button,
  DropdownMenu,
  EmptyState,
  Icon,
  Notice,
  Spinner,
  Tabs,
  TextInput,
} from '@sideroom/ui';
import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react';
import { ApiError } from '../../api/client';
import { usePresenceMap, type Presence } from '../data';
import { useLoungeUi } from '../store';
import {
  acceptFriend,
  addFriendByUsername,
  blockUser,
  ignoreUser,
  removeRelationship,
  type RelationshipRow,
} from './api';
import { useOpenDm } from './dm';
import { friendsOf, incomingOf, outgoingOf, useSocial, wireName } from './social';

type Tab = 'online' | 'all' | 'pending' | 'add';
type PresenceMap = Record<string, Presence>;

interface RowActions {
  run(p: Promise<unknown>, failure: string): Promise<void>;
  message(r: RelationshipRow): void;
}

function FriendRow({
  r,
  presence,
  actions,
}: {
  r: RelationshipRow;
  presence: PresenceMap;
  actions: RowActions;
}) {
  const p = presence[r.id];
  const name = wireName(r.user);
  return (
    <li className="lng-friend-row">
      <Avatar name={name} size={32} status={p?.status ?? 'offline'} />
      <span className="lng-friend-text">
        <span className="lng-friend-name">{name}</span>
        <span className="lng-friend-sub">{p?.activity?.text ?? r.user.username}</span>
      </span>
      <Button
        size="sm"
        variant="secondary"
        aria-label={`Message ${name}`}
        onClick={() => actions.message(r)}
      >
        <Icon name="reply" size={16} /> Message
      </Button>
      <DropdownMenu
        label={`${name} actions`}
        items={[
          { label: 'Start Video Call', disabled: true },
          { label: 'Start Voice Call', disabled: true },
          { type: 'separator' },
          {
            label: 'Remove Friend',
            danger: true,
            onSelect: () =>
              void actions.run(removeRelationship(r.id), 'Could not remove that friend.'),
          },
          {
            label: 'Ignore',
            onSelect: () => void actions.run(ignoreUser(r.id), 'Could not ignore that person.'),
          },
          {
            label: 'Block',
            danger: true,
            onSelect: () => void actions.run(blockUser(r.id), 'Could not block that person.'),
          },
        ]}
        trigger={
          <button type="button" className="lng-icon-btn" aria-label={`More actions for ${name}`}>
            <Icon name="more-horizontal" size={20} />
          </button>
        }
      />
    </li>
  );
}

function PendingRow({
  r,
  kind,
  actions,
}: {
  r: RelationshipRow;
  kind: 'in' | 'out';
  actions: RowActions;
}) {
  const name = wireName(r.user);
  return (
    <li className="lng-friend-row">
      <Avatar name={name} size={32} />
      <span className="lng-friend-text">
        <span className="lng-friend-name">{name}</span>
        <span className="lng-friend-sub">
          {kind === 'in' ? 'Incoming Friend Request' : 'Outgoing Friend Request'}
        </span>
      </span>
      {kind === 'in' && (
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={`Accept ${name}`}
          onClick={() => void actions.run(acceptFriend(r.id), 'Could not accept that request.')}
        >
          <Icon name="check" size={20} />
        </button>
      )}
      <button
        type="button"
        className="lng-icon-btn"
        aria-label={`${kind === 'in' ? 'Ignore' : 'Cancel request to'} ${name}`}
        onClick={() => void actions.run(removeRelationship(r.id), 'Could not update that request.')}
      >
        <Icon name="close" size={20} />
      </button>
    </li>
  );
}

function FriendList({
  list,
  shown,
  heading,
  empty,
  presence,
  actions,
}: {
  list: RelationshipRow[];
  shown: RelationshipRow[];
  heading: string;
  empty: string;
  presence: PresenceMap;
  actions: RowActions;
}) {
  if (!list.length)
    return (
      <EmptyState
        icon={<Icon name="users" size={24} />}
        title={empty}
        description="When you add friends they show up here."
      />
    );
  if (!shown.length)
    return <EmptyState title="No friends match that search" description="Try a different name." />;
  return (
    <>
      <h2 className="lng-friends-h">
        {heading} — {shown.length}
      </h2>
      <ul className="lng-friend-list" aria-label={heading}>
        {shown.map((r) => (
          <FriendRow key={r.id} r={r} presence={presence} actions={actions} />
        ))}
      </ul>
    </>
  );
}

/** Friends page (LNG-FRD-005..014): Online / All / Pending / Add Friend, search, row actions. */
export function FriendsPage() {
  const rows = useSocial((s) => s.relationships);
  const status = useSocial((s) => s.status);
  const load = useSocial((s) => s.loadRelationships);
  const presence = usePresenceMap();
  const openDm = useOpenDm();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const bannerDismissed = useLoungeUi((s) => !!s.dismissedBanners['friends-blocked']);
  const dismissBanner = useLoungeUi((s) => s.dismissBanner);
  const [tab, setTab] = useState<Tab>('online');
  const [query, setQuery] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const friends = friendsOf(rows);
  const incoming = incomingOf(rows);
  const outgoing = outgoingOf(rows);
  const online = friends.filter((r) => {
    const s = presence[r.id]?.status;
    return s !== undefined && s !== 'offline';
  });

  const actions: RowActions = {
    async run(p, failure) {
      setError(null);
      try {
        // The gateway (RELATIONSHIP_*) updates the lists; no re-fetch (D-183).
        await p;
      } catch (e) {
        setError(e instanceof ApiError && e.message ? e.message : failure);
      }
    },
    message(r) {
      openDm([r.id]).catch(() => setError('Could not open that conversation.'));
    },
  };

  const q = query.trim().toLowerCase();
  const match = (r: RelationshipRow) =>
    !q || wireName(r.user).toLowerCase().includes(q) || r.user.username.toLowerCase().includes(q);

  const active = useMemo(() => activeNow(friends, presence), [friends, presence]);

  let body: ReactNode;
  if (tab === 'add') body = <AddFriend />;
  else if (tab === 'pending')
    body =
      incoming.length + outgoing.length === 0 ? (
        <EmptyState
          icon={<Icon name="clock" size={24} />}
          title="There are no pending friend requests"
          description="Here's a friend-shaped gap. Add someone from the Add Friend tab."
        />
      ) : (
        <>
          {incoming.length > 0 && (
            <>
              <h2 className="lng-friends-h">Incoming — {incoming.length}</h2>
              <ul className="lng-friend-list" aria-label="Incoming friend requests">
                {incoming.filter(match).map((r) => (
                  <PendingRow key={r.id} r={r} kind="in" actions={actions} />
                ))}
              </ul>
            </>
          )}
          {outgoing.length > 0 && (
            <>
              <h2 className="lng-friends-h">Outgoing — {outgoing.length}</h2>
              <ul className="lng-friend-list" aria-label="Outgoing friend requests">
                {outgoing.filter(match).map((r) => (
                  <PendingRow key={r.id} r={r} kind="out" actions={actions} />
                ))}
              </ul>
            </>
          )}
        </>
      );
  else if (tab === 'online')
    body = (
      <FriendList
        list={online}
        shown={online.filter(match)}
        heading="Online"
        empty="No one's around to play with"
        presence={presence}
        actions={actions}
      />
    );
  else
    body = (
      <FriendList
        list={friends}
        shown={friends.filter(match)}
        heading="All friends"
        empty="You have no friends yet"
        presence={presence}
        actions={actions}
      />
    );

  const tabItems = [
    { id: 'online', label: 'Online' },
    { id: 'all', label: 'All' },
    {
      id: 'pending',
      label: (
        <>
          Pending{' '}
          {incoming.length > 0 && (
            <Badge variant="danger" pill>
              {incoming.length}
            </Badge>
          )}
        </>
      ),
    },
    { id: 'add', label: 'Add Friend' },
  ].map((t) => ({ ...t, content: null }));

  return (
    <section className="lng-channel-view lng-friends" aria-label="Friends">
      <header className="lng-header">
        <Icon name="users" size={24} />
        <h1 className="lng-header-title">Friends</h1>
        <Tabs
          items={tabItems}
          label="Friends sections"
          value={tab}
          onValueChange={(id) => setTab(id as Tab)}
          variant="pill"
          className="lng-friends-tabs"
        />
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="New Group DM"
          onClick={() => openDialog({ kind: 'createDm' })}
        >
          <Icon name="plus" size={20} />
        </button>
      </header>
      <div className="lng-friends-body">
        <div className="lng-friends-main">
          {error && (
            <Notice variant="danger" onDismiss={() => setError(null)}>
              {error}
            </Notice>
          )}
          {tab !== 'add' && !bannerDismissed && (
            <Notice variant="info" onDismiss={() => dismissBanner('friends-blocked')}>
              Looking for accounts you&apos;ve blocked or ignored? Find them in User Settings under
              Privacy & Safety.
            </Notice>
          )}
          {tab !== 'add' && (
            <TextInput
              label="Search friends"
              value={query}
              placeholder="Search"
              onChange={(e) => setQuery(e.target.value)}
            />
          )}
          {status === 'loading' && rows.length === 0 && (
            <p role="status">
              <Spinner label="Loading friends" />
            </p>
          )}
          {status === 'error' && (
            <Notice variant="danger">
              Could not load your friends.{' '}
              <Button size="sm" variant="secondary" onClick={() => void load()}>
                Try again
              </Button>
            </Notice>
          )}
          {(status === 'ready' || rows.length > 0 || tab === 'add') && body}
        </div>
        <aside className="lng-active-now" aria-label="Active Now">
          <h2 className="lng-friends-h">Active Now</h2>
          {active.length === 0 ? (
            <div className="lng-active-empty">
              <p className="lng-active-title">It&apos;s quiet for now...</p>
              <p className="lng-muted">
                When a friend starts an activity, like playing, watching or reading, we&apos;ll show
                it here.
              </p>
            </div>
          ) : (
            <ul className="lng-active-list">
              {active.map((a) => (
                <li key={a.row.id} className="lng-active-card">
                  <Avatar name={wireName(a.row.user)} size={32} status={a.status} />
                  <span className="lng-friend-text">
                    <span className="lng-friend-name">{wireName(a.row.user)}</span>
                    <span className="lng-friend-sub">{a.text}</span>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </aside>
      </div>
    </section>
  );
}

/** Friends who are doing something (an activity that is not just a custom status). */
export function activeNow(friends: RelationshipRow[], presence: PresenceMap) {
  return friends.flatMap((row) => {
    const p = presence[row.id];
    const act = p?.activity;
    if (!p || p.status === 'offline' || !act || act.kind === 'custom') return [];
    const verb =
      act.kind === 'playing'
        ? 'Playing'
        : act.kind === 'watching'
          ? 'Watching'
          : act.kind === 'reading'
            ? 'Reading'
            : 'Listening to';
    return [{ row, status: p.status, text: `${verb} ${act.text}` }];
  });
}

function AddFriend() {
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const username = name.trim().replace(/^@/, '');
    if (!username) {
      setResult({ ok: false, text: 'Enter a username.' });
      return;
    }
    setBusy(true);
    setResult(null);
    try {
      await addFriendByUsername(username);
      setResult({ ok: true, text: `Success! Your friend request to ${username} was sent.` });
      setName('');
    } catch (err) {
      setResult({
        ok: false,
        text:
          err instanceof ApiError && err.status === 429
            ? 'You are sending requests too quickly. Wait a moment and try again.'
            : 'Hm, didn’t work. Double check that the username is correct.',
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="lng-form lng-add-friend" onSubmit={(e) => void submit(e)} noValidate>
      <h2 className="lng-friends-h">Add Friend</h2>
      <p className="lng-muted">You can add friends with their SIDE ROOM username.</p>
      <TextInput
        label="Username"
        value={name}
        placeholder="You can add friends with their username"
        onChange={(e) => setName(e.target.value)}
        {...(result && !result.ok ? { error: result.text } : {})}
      />
      {result?.ok && (
        <p role="status" className="lng-add-ok">
          {result.text}
        </p>
      )}
      <div>
        <Button type="submit" loading={busy} disabled={!name.trim()}>
          Send Friend Request
        </Button>
      </div>
    </form>
  );
}
