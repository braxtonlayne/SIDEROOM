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
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { ApiError } from '../../api/client';
import { FriendShelfShare } from '../../settings/user/storage/FriendShelfShare';
import { useT } from '../../i18n';
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
import { ActiveNow } from './ActiveNow';
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
  const t = useT();
  const p = presence[r.id];
  const name = wireName(r.user);
  const [shareOpen, setShareOpen] = useState(false);
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
        aria-label={t('lounge.friends.messageLabel', { name })}
        onClick={() => actions.message(r)}
      >
        <Icon name="reply" size={16} /> {t('lounge.friends.message')}
      </Button>
      <DropdownMenu
        label={t('lounge.friends.actionsLabel', { name })}
        items={[
          { label: t('lounge.friends.videoCall'), disabled: true },
          { label: t('lounge.friends.voiceCall'), disabled: true },
          {
            label: t('settings.storageShare.menu'),
            onSelect: () => setShareOpen((v) => !v),
          },
          { type: 'separator' },
          {
            label: t('lounge.friends.removeFriend'),
            danger: true,
            onSelect: () =>
              void actions.run(removeRelationship(r.id), t('lounge.friends.removeError')),
          },
          {
            label: t('lounge.friends.ignore'),
            onSelect: () => void actions.run(ignoreUser(r.id), t('lounge.friends.ignoreError')),
          },
          {
            label: t('lounge.friends.block'),
            danger: true,
            onSelect: () => void actions.run(blockUser(r.id), t('lounge.friends.blockError')),
          },
        ]}
        trigger={
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={t('lounge.friends.moreActions', { name })}
          >
            <Icon name="more-horizontal" size={20} />
          </button>
        }
      />
      {shareOpen && (
        <div className="sto-share-row">
          <FriendShelfShare userId={r.id} showEmpty />
        </div>
      )}
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
  const t = useT();
  const name = wireName(r.user);
  return (
    <li className="lng-friend-row">
      <Avatar name={name} size={32} />
      <span className="lng-friend-text">
        <span className="lng-friend-name">{name}</span>
        <span className="lng-friend-sub">
          {kind === 'in'
            ? t('lounge.friends.incomingRequest')
            : t('lounge.friends.outgoingRequest')}
        </span>
      </span>
      {kind === 'in' && (
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.friends.acceptLabel', { name })}
          onClick={() => void actions.run(acceptFriend(r.id), t('lounge.friends.acceptError'))}
        >
          <Icon name="check" size={20} />
        </button>
      )}
      <button
        type="button"
        className="lng-icon-btn"
        aria-label={
          kind === 'in'
            ? t('lounge.friends.ignoreRequestLabel', { name })
            : t('lounge.friends.cancelRequestLabel', { name })
        }
        onClick={() => void actions.run(removeRelationship(r.id), t('lounge.friends.updateError'))}
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
  const t = useT();
  if (!list.length)
    return (
      <EmptyState
        icon={<Icon name="users" size={24} />}
        title={empty}
        description={t('lounge.friends.listEmptyBody')}
      />
    );
  if (!shown.length)
    return (
      <EmptyState
        title={t('lounge.friends.noMatchTitle')}
        description={t('lounge.friends.noMatchBody')}
      />
    );
  return (
    <>
      <h2 className="lng-friends-h">
        {t('lounge.friends.headingCount', { heading, count: shown.length })}
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
  const t = useT();
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
      openDm([r.id]).catch(() => setError(t('lounge.friends.openError')));
    },
  };

  const q = query.trim().toLowerCase();
  const match = (r: RelationshipRow) =>
    !q || wireName(r.user).toLowerCase().includes(q) || r.user.username.toLowerCase().includes(q);

  let body: ReactNode;
  if (tab === 'add') body = <AddFriend />;
  else if (tab === 'pending')
    body =
      incoming.length + outgoing.length === 0 ? (
        <EmptyState
          icon={<Icon name="clock" size={24} />}
          title={t('lounge.friends.pendingEmptyTitle')}
          description={t('lounge.friends.pendingEmptyBody')}
        />
      ) : (
        <>
          {incoming.length > 0 && (
            <>
              <h2 className="lng-friends-h">
                {t('lounge.friends.incomingCount', { count: incoming.length })}
              </h2>
              <ul className="lng-friend-list" aria-label={t('lounge.friends.incomingList')}>
                {incoming.filter(match).map((r) => (
                  <PendingRow key={r.id} r={r} kind="in" actions={actions} />
                ))}
              </ul>
            </>
          )}
          {outgoing.length > 0 && (
            <>
              <h2 className="lng-friends-h">
                {t('lounge.friends.outgoingCount', { count: outgoing.length })}
              </h2>
              <ul className="lng-friend-list" aria-label={t('lounge.friends.outgoingList')}>
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
        heading={t('lounge.friends.tabOnline')}
        empty={t('lounge.friends.onlineEmpty')}
        presence={presence}
        actions={actions}
      />
    );
  else
    body = (
      <FriendList
        list={friends}
        shown={friends.filter(match)}
        heading={t('lounge.friends.allFriends')}
        empty={t('lounge.friends.allEmpty')}
        presence={presence}
        actions={actions}
      />
    );

  const tabItems = [
    { id: 'online', label: t('lounge.friends.tabOnline') },
    { id: 'all', label: t('lounge.friends.tabAll') },
    {
      id: 'pending',
      label: (
        <>
          {t('lounge.friends.tabPending')}{' '}
          {incoming.length > 0 && (
            <Badge variant="danger" pill>
              {incoming.length}
            </Badge>
          )}
        </>
      ),
    },
    { id: 'add', label: t('lounge.friends.tabAdd') },
  ].map((item) => ({ ...item, content: null }));

  return (
    <section className="lng-channel-view lng-friends" aria-label={t('lounge.friends.title')}>
      <header className="lng-header">
        <Icon name="users" size={24} />
        <h1 className="lng-header-title">{t('lounge.friends.title')}</h1>
        <Tabs
          items={tabItems}
          label={t('lounge.friends.sections')}
          value={tab}
          onValueChange={(id) => setTab(id as Tab)}
          variant="pill"
          className="lng-friends-tabs"
        />
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.friends.newGroupDm')}
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
              {t('lounge.friends.blockedHint')}
            </Notice>
          )}
          {tab !== 'add' && (
            <TextInput
              label={t('lounge.friends.searchLabel')}
              value={query}
              placeholder={t('lounge.friends.searchPlaceholder')}
              onChange={(e) => setQuery(e.target.value)}
            />
          )}
          {status === 'loading' && rows.length === 0 && (
            <p role="status">
              <Spinner label={t('lounge.friends.loading')} />
            </p>
          )}
          {status === 'error' && (
            <Notice variant="danger">
              {t('lounge.friends.loadError')}{' '}
              <Button size="sm" variant="secondary" onClick={() => void load()}>
                {t('lounge.friends.tryAgain')}
              </Button>
            </Notice>
          )}
          {(status === 'ready' || rows.length > 0 || tab === 'add') && body}
        </div>
        <ActiveNow friends={friends} />
      </div>
    </section>
  );
}

function AddFriend() {
  const t = useT();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const username = name.trim().replace(/^@/, '');
    if (!username) {
      setResult({ ok: false, text: t('lounge.friends.enterUsername') });
      return;
    }
    setBusy(true);
    setResult(null);
    try {
      await addFriendByUsername(username);
      setResult({ ok: true, text: t('lounge.friends.requestSent', { username }) });
      setName('');
    } catch (err) {
      setResult({
        ok: false,
        text:
          err instanceof ApiError && err.status === 429
            ? t('lounge.friends.rateLimited')
            : t('lounge.friends.addFailed'),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="lng-form lng-add-friend" onSubmit={(e) => void submit(e)} noValidate>
      <h2 className="lng-friends-h">{t('lounge.friends.tabAdd')}</h2>
      <p className="lng-muted">{t('lounge.friends.addHint')}</p>
      <TextInput
        label={t('lounge.friends.usernameLabel')}
        value={name}
        placeholder={t('lounge.friends.usernamePlaceholder')}
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
          {t('lounge.friends.sendRequest')}
        </Button>
      </div>
    </form>
  );
}
