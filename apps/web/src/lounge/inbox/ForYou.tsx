import { useNavigate } from '@tanstack/react-router';
import { useCallback, useEffect, useState } from 'react';
import { formatDateTime, useT, type MessageKey } from '../../i18n';
import { getUpdates } from '../../library/api';
import type { UpdateItem } from '../../library/types';
import { listMine } from '../../review/api';
import type { Submission } from '../../review/types';
import { listMemberInvites, type MemberInvite } from '../../settings/user/api';
import { acceptFriend, listRelationships, REL, removeRelationship } from '../shell/api';
import type { RelationshipRow } from '../shell/api';

const LIMIT = 5;
const DECIDED = new Set(['approved', 'rejected', 'changes_requested', 'published']);

interface Data {
  requests: RelationshipRow[];
  chapters: UpdateItem[];
  decisions: Submission[];
  invites: MemberInvite[];
}

const STATE_KEY: Record<string, MessageKey> = {
  approved: 'lounge.inbox.forYouStateApproved',
  rejected: 'lounge.inbox.forYouStateRejected',
  changes_requested: 'lounge.inbox.forYouStateChanges',
  published: 'lounge.inbox.forYouStatePublished',
};

const name = (u: { username: string; global_name?: string | null }) => u.global_name || u.username;

/**
 * "For You" tab of the inbox (LNG-NTF-006): incoming friend requests, new chapters of followed series, decisions on
 * the member's Library submissions and invite codes that were redeemed. There is no combined feed endpoint, so each
 * part is read from the route that owns it; a part that fails to load is left out instead of failing the tab.
 */
export function ForYou({ onGo }: { onGo: () => void }) {
  const t = useT();
  const navigate = useNavigate();
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    let live = true;
    void Promise.allSettled([
      listRelationships(),
      getUpdates({ following: true, limit: LIMIT }),
      listMine({ sort: 'updated', order: 'desc', limit: 20 }),
      listMemberInvites(),
    ]).then(([rel, upd, subs, inv]) => {
      if (!live) return;
      const all = [rel, upd, subs, inv];
      if (all.every((r) => r.status === 'rejected')) {
        setError(t('lounge.inbox.loadFailed'));
        return;
      }
      setData({
        requests:
          rel.status === 'fulfilled' ? rel.value.filter((r) => r.type === REL.INCOMING) : [],
        chapters: upd.status === 'fulfilled' ? upd.value.items.slice(0, LIMIT) : [],
        decisions:
          subs.status === 'fulfilled'
            ? subs.value.items.filter((s) => DECIDED.has(s.state)).slice(0, LIMIT)
            : [],
        invites: inv.status === 'fulfilled' ? inv.value.redeemed.slice(0, LIMIT) : [],
      });
    });
    return () => {
      live = false;
    };
  }, [t]);
  useEffect(() => load(), [load]);

  if (error) return <p className="lng-muted">{error}</p>;
  if (!data) return <p className="lng-muted">{t('lounge.inbox.loading')}</p>;
  const empty =
    !data.requests.length &&
    !data.chapters.length &&
    !data.decisions.length &&
    !data.invites.length;
  if (empty) return <p className="lng-muted">{t('lounge.inbox.forYouEmpty')}</p>;

  const go = (to: string) => {
    void navigate({ to });
    onGo();
  };
  const dropRequest = (id: string) =>
    setData((d) => (d ? { ...d, requests: d.requests.filter((r) => r.user.id !== id) } : d));

  return (
    <div className="lng-inbox-body">
      {data.requests.length > 0 && (
        <section aria-label={t('lounge.inbox.forYouRequests')}>
          <h3 className="lng-inbox-head">{t('lounge.inbox.forYouRequests')}</h3>
          <ul className="lng-inbox-list">
            {data.requests.map((r) => (
              <li key={r.user.id} className="lng-inbox-row">
                <span className="lng-inbox-item">
                  <strong>{name(r.user)}</strong>
                  <span className="lng-muted"> {t('lounge.inbox.forYouWantsFriend')}</span>
                </span>
                <button
                  type="button"
                  className="lng-inbox-mark"
                  aria-label={t('lounge.inbox.forYouAccept', { name: name(r.user) })}
                  onClick={() => void acceptFriend(r.user.id).then(() => dropRequest(r.user.id))}
                >
                  {t('lounge.inbox.forYouAcceptShort')}
                </button>
                <button
                  type="button"
                  className="lng-inbox-mark"
                  aria-label={t('lounge.inbox.forYouDecline', { name: name(r.user) })}
                  onClick={() =>
                    void removeRelationship(r.user.id).then(() => dropRequest(r.user.id))
                  }
                >
                  {t('lounge.inbox.forYouDeclineShort')}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {data.chapters.length > 0 && (
        <section aria-label={t('lounge.inbox.forYouChapters')}>
          <h3 className="lng-inbox-head">{t('lounge.inbox.forYouChapters')}</h3>
          <ul className="lng-inbox-list">
            {data.chapters.map((c) => (
              <li key={c.chapter_id}>
                <button
                  type="button"
                  className="lng-inbox-item"
                  onClick={() => go(`/library/r/${encodeURIComponent(c.chapter_id)}`)}
                >
                  <strong>{c.series_title}</strong>
                  <span className="lng-muted">
                    {' '}
                    {t('lounge.inbox.forYouChapter', { n: c.chapter_number })}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {data.decisions.length > 0 && (
        <section aria-label={t('lounge.inbox.forYouDecisions')}>
          <h3 className="lng-inbox-head">{t('lounge.inbox.forYouDecisions')}</h3>
          <ul className="lng-inbox-list">
            {data.decisions.map((s) => (
              <li key={s.id}>
                <button
                  type="button"
                  className="lng-inbox-item"
                  onClick={() => go(`/library/submissions/${encodeURIComponent(s.id)}`)}
                >
                  <strong>{s.title}</strong>
                  <span className="lng-muted">
                    {' '}
                    {t(STATE_KEY[s.state] ?? STATE_KEY.published!)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {data.invites.length > 0 && (
        <section aria-label={t('lounge.inbox.forYouInvites')}>
          <h3 className="lng-inbox-head">{t('lounge.inbox.forYouInvites')}</h3>
          <ul className="lng-inbox-list">
            {data.invites.map((i) => (
              <li key={i.code}>
                <span className="lng-inbox-item">
                  <strong>{i.redeemed_by?.username ?? t('lounge.inbox.forYouSomeone')}</strong>
                  <span className="lng-muted">
                    {' '}
                    {t('lounge.inbox.forYouJoined')}
                    {i.redeemed_at_ms ? ` · ${formatDateTime(i.redeemed_at_ms)}` : ''}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
