import { Avatar, Button, EmptyState, Icon, Modal, Tabs } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { formatDate, useT } from '../../i18n';
import { cdnUrl } from '../../store/cdn';
import { useCurrentUser, useGuilds, useUsers } from '../data';
import { snowflakeTime } from '../liveMap';
import { Md } from '../msg/Md';
import { useOpenDm } from '../shell/dm';
import { Badges } from './Badges';
import { listUserLists, listUserReviews, type UserList, type UserReview } from './extras';
import {
  ActivityList,
  FriendAction,
  NoteField,
  ProfileAvatar,
  ProfileBanner,
  ProfileMoreMenu,
  StatusBubble,
  Themed,
} from './parts';
import type { ProfileTarget } from './popoutStore';
import { usePopout } from './popoutStore';
import { useProfileView } from './profileView';
import { statsText } from './useProfile';

const date = (iso: string | null | undefined) =>
  iso && !Number.isNaN(Date.parse(iso)) ? formatDate(new Date(iso), { dateStyle: 'medium' }) : null;

/** Only http(s) links are ever followed from a connection. */
const safeUrl = (u: string | null | undefined) => (u && /^https?:\/\//i.test(u) ? u : null);

type Load<T> = { kind: 'loading' } | { kind: 'ready'; items: T[] } | { kind: 'error' };

function useLoaded<T>(fetcher: () => Promise<T[]>, key: string): Load<T> {
  const [state, setState] = useState<Load<T>>({ kind: 'loading' });
  useEffect(() => {
    let live = true;
    setState({ kind: 'loading' });
    fetcher().then(
      (items) => live && setState({ kind: 'ready', items }),
      () => live && setState({ kind: 'error' }),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return state;
}

function WatchingTab({
  userId,
  view,
}: {
  userId: string;
  view: ReturnType<typeof useProfileView>;
}) {
  const t = useT();
  const lists = useLoaded<UserList>(() => listUserLists(userId), userId);
  const now = (view.presence.activities ?? []).filter(
    (a) => a.kind === 'watching' || a.kind === 'reading',
  );
  return (
    <div className="prf-tab">
      {now.length > 0 && (
        <section>
          <h3 className="lng-pop-h">{t('lounge.fullProfile.nowHeading')}</h3>
          <ActivityList activities={now} />
        </section>
      )}
      <section>
        <h3 className="lng-pop-h">{t('lounge.fullProfile.listsHeading')}</h3>
        {lists.kind === 'ready' && lists.items.length > 0 ? (
          <ul className="prf-lists">
            {lists.items.map((l) => (
              <li key={l.id}>
                <strong>{l.name}</strong>
                <span className="lng-muted">
                  {t('lounge.fullProfile.listCount', { count: l.item_count })}
                </span>
              </li>
            ))}
          </ul>
        ) : lists.kind === 'loading' ? (
          <p className="lng-muted" role="status">
            {t('lounge.profilePopout.loading')}
          </p>
        ) : now.length === 0 ? (
          <EmptyState
            title={t('lounge.fullProfile.watchingEmptyTitle')}
            description={t('lounge.fullProfile.watchingEmptyBody')}
          />
        ) : null}
      </section>
    </div>
  );
}

function ReviewsTab({ userId }: { userId: string }) {
  const t = useT();
  const reviews = useLoaded<UserReview>(() => listUserReviews(userId), userId);
  if (reviews.kind === 'loading')
    return (
      <p className="lng-muted" role="status">
        {t('lounge.profilePopout.loading')}
      </p>
    );
  if (reviews.kind === 'error' || reviews.items.length === 0)
    return (
      <EmptyState
        title={t('lounge.fullProfile.reviewsEmptyTitle')}
        description={t('lounge.fullProfile.reviewsEmptyBody')}
      />
    );
  return (
    <ul className="prf-lists">
      {reviews.items.map((r) => (
        <li key={r.id}>
          <strong>{r.title}</strong>
          {r.rating != null && (
            <span className="lng-muted">
              {t('lounge.fullProfile.rating', { rating: r.rating })}
            </span>
          )}
          {r.body && <p className="prf-review">{r.body}</p>}
        </li>
      ))}
    </ul>
  );
}

/** The full profile (LNG-PRF-004..006): header, left column of facts, right tabs. */
export function FullProfile({ target, onClose }: { target: ProfileTarget; onClose: () => void }) {
  const t = useT();
  const me = useCurrentUser();
  const guilds = useGuilds();
  const users = useUsers();
  const navigate = useNavigate();
  const openDm = useOpenDm();
  const openFull = usePopout((s) => s.openFull);
  const view = useProfileView(target.userId, target.guildId);
  const { profile } = view;
  const isSelf = target.userId === me.id;
  const isBot = !!users[target.userId]?.bot || !!profile?.user.bot;
  const [dmError, setDmError] = useState(false);

  const memberSince =
    date(profile?.member_since ?? profile?.created_at) ?? date(snowflakeTime(target.userId));
  const joinedHere = target.guildId
    ? date(
        profile?.mutual_guilds?.find((g) => g.id === target.guildId)?.member_since ??
          profile?.guild_member_profile?.joined_at,
      )
    : null;
  const here = guilds.find((g) => g.id === target.guildId);
  const friendsCount = profile?.mutual_friends_count ?? profile?.mutual_friends?.length ?? 0;
  const mutualGuilds = profile?.mutual_guilds ?? [];
  const guildName = (id: string) =>
    guilds.find((x) => x.id === id)?.name ?? t('lounge.profilePopout.aServer');

  const message = () => {
    setDmError(false);
    openDm([target.userId]).then(onClose, () => setDmError(true));
  };

  const tabs = [
    {
      id: 'activity',
      label: t('lounge.fullProfile.tabActivity'),
      content: (
        <div className="prf-tab">
          {view.presence.activities && view.presence.activities.length > 0 ? (
            <ActivityList activities={view.presence.activities} />
          ) : (
            <EmptyState
              title={t('lounge.fullProfile.activityEmptyTitle')}
              description={isSelf ? undefined : t('lounge.fullProfile.activityEmptyBody')}
              action={
                !isSelf && !isBot ? (
                  <Button size="sm" onClick={message}>
                    {t('lounge.fullProfile.message')}
                  </Button>
                ) : undefined
              }
            />
          )}
        </div>
      ),
    },
    {
      id: 'friends',
      label: t('lounge.fullProfile.tabFriends', { count: friendsCount }),
      content: (
        <div className="prf-tab">
          {(profile?.mutual_friends ?? []).length === 0 ? (
            <EmptyState title={t('lounge.fullProfile.friendsEmpty')} />
          ) : (
            <ul className="prf-people">
              {(profile?.mutual_friends ?? []).map((f) => {
                const name = f.global_name || f.username || t('lounge.profilePopout.unknown');
                return (
                  <li key={f.id}>
                    <button
                      type="button"
                      className="prf-person"
                      onClick={() => openFull({ userId: f.id })}
                    >
                      <Avatar
                        name={name}
                        src={
                          cdnUrl('avatars', f.id, f.avatar, { size: 64 }) ?? users[f.id]?.avatarUrl
                        }
                        size={32}
                      />
                      <span>{name}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      ),
    },
    {
      id: 'servers',
      label: t('lounge.fullProfile.tabServers', { count: mutualGuilds.length }),
      content: (
        <div className="prf-tab">
          {mutualGuilds.length === 0 ? (
            <EmptyState title={t('lounge.fullProfile.serversEmpty')} />
          ) : (
            <ul className="prf-people">
              {mutualGuilds.map((g) => {
                const name = guildName(g.id);
                const guild = guilds.find((x) => x.id === g.id);
                return (
                  <li key={g.id}>
                    <button
                      type="button"
                      className="prf-person"
                      onClick={() => {
                        onClose();
                        void navigate({ to: '/channels/$guildId', params: { guildId: g.id } });
                      }}
                    >
                      <Avatar name={name} src={guild?.iconUrl ?? null} size={32} />
                      <span>
                        <strong>{name}</strong>
                        {g.nick && (
                          <span className="lng-muted">
                            {' '}
                            {t('lounge.profilePopout.asNick', { nick: g.nick })}
                          </span>
                        )}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      ),
    },
    {
      id: 'watching',
      label: t('lounge.fullProfile.tabWatching'),
      content: <WatchingTab userId={target.userId} view={view} />,
    },
    {
      id: 'reviews',
      label: t('lounge.fullProfile.tabReviews'),
      content: <ReviewsTab userId={target.userId} />,
    },
  ];

  return (
    <Modal open onClose={onClose} title={view.name} size="lg">
      <Themed view={view} className="prf-full">
        <ProfileBanner view={view} tall />
        <div className="prf-full-head">
          <div className="prf-full-avatar">
            <ProfileAvatar view={view} />
          </div>
          <div className="prf-full-id">
            <p className="lng-pop-handle">
              {view.handle}
              {view.pronouns ? ` • ${view.pronouns}` : ''}
            </p>
            <Badges badges={view.badges} />
          </div>
          <div className="prf-full-actions">
            {!isSelf && !isBot && (
              <Button size="sm" onClick={message}>
                {t('lounge.fullProfile.message')}
              </Button>
            )}
            {!isSelf && <FriendAction userId={target.userId} isBot={isBot} />}
            <ProfileMoreMenu
              userId={target.userId}
              name={view.name}
              username={view.handle}
              isSelf={isSelf}
            />
          </div>
        </div>
        {dmError && (
          <p role="alert" className="lng-failed">
            {t('lounge.profilePopout.sendError')}
          </p>
        )}
        {view.loading && (
          <p className="lng-muted" role="status">
            {t('lounge.profilePopout.loading')}
          </p>
        )}
        {view.failed && <p role="alert">{t('lounge.profilePopout.loadError')}</p>}
        <StatusBubble activity={view.presence.custom} />
        <div className="prf-full-cols">
          <aside className="prf-left" aria-label={t('lounge.fullProfile.about')}>
            {view.bio && (
              <section>
                <h3 className="lng-pop-h">{t('lounge.profilePopout.aboutMe')}</h3>
                <div className="lng-pop-bio">
                  <Md text={view.bio} />
                </div>
              </section>
            )}
            <section>
              <h3 className="lng-pop-h">{t('lounge.profilePopout.memberSince')}</h3>
              <p>{memberSince}</p>
              {joinedHere && here && (
                <p className="lng-muted">
                  {t('lounge.fullProfile.joinedHere', { name: here.name, date: joinedHere })}
                </p>
              )}
            </section>
            {date(profile?.friends_since) && (
              <section>
                <h3 className="lng-pop-h">{t('lounge.fullProfile.friendsSinceHeading')}</h3>
                <p>{date(profile?.friends_since)}</p>
              </section>
            )}
            {(profile?.connected_accounts.length ?? 0) > 0 && (
              <section>
                <h3 className="lng-pop-h">{t('lounge.profilePopout.connections')}</h3>
                <ul className="prf-conns">
                  {profile!.connected_accounts.map((c) => {
                    const href = safeUrl((c as { url?: string | null }).url);
                    return (
                      <li key={`${c.type}-${c.id}`}>
                        <Icon name="link" size={16} />
                        <span className="prf-conn-text">
                          <span>
                            {c.name} <span className="lng-muted">({c.type})</span>
                            {c.verified && (
                              <Icon
                                name="check"
                                size={16}
                                label={t('lounge.profilePopout.verified')}
                              />
                            )}
                          </span>
                          {statsText(c.stats) && (
                            <span className="lng-muted">{statsText(c.stats)}</span>
                          )}
                        </span>
                        {href && (
                          <a
                            href={href}
                            target="_blank"
                            rel="noopener noreferrer nofollow"
                            aria-label={t('lounge.fullProfile.openConnection', { name: c.name })}
                          >
                            <Icon name="arrow-up-right" size={16} />
                          </a>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}
            {!isSelf && <NoteField userId={target.userId} />}
          </aside>
          <section className="prf-right">
            <Tabs items={tabs} label={t('lounge.fullProfile.tabs')} />
          </section>
        </div>
      </Themed>
    </Modal>
  );
}
