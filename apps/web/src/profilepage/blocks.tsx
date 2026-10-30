// The stateful blocks of a profile page in the app (EXT-PRF-003, EXT-PRF-004): the guestbook with its form and
// the presence-driven "now" blocks. The pure blocks (and the new ones of PRF-PRO-001) live in
// @sideroom/profile-blocks, which the public profile surface uses too (D-385); the app fills the slots here.
import { Avatar, Button, TextArea } from '@sideroom/ui';
import type { BlockSlots } from '@sideroom/profile-blocks';
import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../api/client';
import { formatDate, useT } from '../i18n';
import { usePresence } from '../lounge/data';
import { ActivityList } from '../lounge/profile/parts';
import { cdnUrl } from '../store/cdn';
import { approveGuestbookEntry, listGuestbook, removeGuestbookEntry, signGuestbook } from './api';
import type { Block, GuestbookEntry, PageUser, ProfilePage } from './types';

const text = (c: Record<string, unknown>, k: string): string => {
  const v = c[k];
  return typeof v === 'string' ? v : '';
};

const nameOf = (u: PageUser) => u.display_name?.trim() || u.username;
const avatarOf = (u: PageUser, size = 64) => cdnUrl('avatars', u.id, u.avatar, { size });

/** Presence for a "now" block. It reads presence like any other surface, so the member's own presence privacy
 *  applies on top of the block's audience. */
function usePresenceLines(page: ProfilePage, kinds: readonly string[]) {
  const presence = usePresence(page.user.id);
  return (presence.activities ?? []).filter((a) => kinds.includes(a.kind));
}

function NowWatching({ page }: { page: ProfilePage }) {
  const t = useT();
  const now = usePresenceLines(page, ['watching', 'reading']);
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.nowTitle')}</h3>
      {now.length > 0 ? (
        <ActivityList activities={now} />
      ) : (
        <p className="pp-muted">{t('profile.blocks.playingEmpty')}</p>
      )}
    </>
  );
}

function NowPlaying({ page }: { page: ProfilePage }) {
  const t = useT();
  const now = usePresenceLines(page, ['listening', 'playing']);
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.playingTitle')}</h3>
      {now.length > 0 ? (
        <ActivityList activities={now} />
      ) : (
        <p className="pp-muted">{t('profile.blocks.playingEmpty')}</p>
      )}
    </>
  );
}

/** The guestbook (EXT-PRF-004): friends sign, the owner removes any entry, an author removes their own. */
export function Guestbook({ page, block }: { page: ProfilePage; block: Block }) {
  const t = useT();
  const owner = page.user.id;
  const [entries, setEntries] = useState<GuestbookEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(() => {
    listGuestbook(owner).then(setEntries, () =>
      setError(t('lounge.profilePage.guestbookLoadFailed')),
    );
  }, [owner, t]);
  useEffect(refresh, [refresh]);

  const sign = async () => {
    setBusy(true);
    setError(null);
    try {
      const e = await signGuestbook(owner, draft);
      setEntries((cur) => [e, ...(cur ?? [])]);
      setDraft('');
    } catch (e) {
      setError(
        e instanceof ApiError && e.message
          ? e.message
          : t('lounge.profilePage.guestbookSignFailed'),
      );
    } finally {
      setBusy(false);
    }
  };
  const approve = async (id: string) => {
    try {
      await approveGuestbookEntry(id);
      setEntries((cur) => (cur ?? []).map((x) => (x.id === id ? { ...x, pending: false } : x)));
    } catch {
      setError(t('lounge.profilePage.guestbookSignFailed'));
    }
  };
  const remove = async (id: string) => {
    try {
      await removeGuestbookEntry(owner, id);
      setEntries((cur) => (cur ?? []).filter((x) => x.id !== id));
    } catch {
      setError(t('lounge.profilePage.guestbookRemoveFailed'));
    }
  };

  return (
    <>
      <h3 className="pp-title">
        {text(block.config, 'title') || t('lounge.profilePage.guestbookTitle')}
      </h3>
      {page.is_owner && typeof page.guestbook_count === 'number' && (
        <p className="pp-muted">
          {t('lounge.profilePage.guestbookCount', {
            count: page.guestbook_count,
            limit: page.guestbook_limit ?? 500,
          })}
        </p>
      )}
      {page.can_sign && (
        <form
          className="pp-sign"
          onSubmit={(e) => {
            e.preventDefault();
            if (draft.trim()) void sign();
          }}
        >
          <TextArea
            label={t('lounge.profilePage.guestbookLabel')}
            value={draft}
            maxLength={500}
            rows={2}
            onChange={(e) => setDraft(e.target.value)}
          />
          <Button type="submit" size="sm" variant="primary" loading={busy} disabled={!draft.trim()}>
            {t('lounge.profilePage.guestbookSign')}
          </Button>
        </form>
      )}
      {error && (
        <p role="alert" className="pp-error">
          {error}
        </p>
      )}
      {entries && entries.length === 0 && (
        <p className="pp-muted">{t('lounge.profilePage.guestbookEmpty')}</p>
      )}
      <ul className="pp-entries" aria-label={t('lounge.profilePage.guestbookTitle')}>
        {(entries ?? []).map((e) => (
          <li key={e.id}>
            <Avatar
              name={e.author ? nameOf(e.author) : t('profile.public.visitor')}
              src={e.author ? avatarOf(e.author) : null}
              size={32}
            />
            <div>
              <p className="pp-muted">
                {e.author ? (
                  <a className="pp-link" href={`/u/${encodeURIComponent(e.author.username)}`}>
                    {nameOf(e.author)}
                  </a>
                ) : (
                  t('profile.public.visitor')
                )}
                {e.pending && <strong> {t('profile.public.pending')}</strong>}{' '}
                {formatDate(new Date(e.created_at_ms), { dateStyle: 'medium' })}
              </p>
              <p className="pp-text">{e.body}</p>
            </div>
            {e.pending && page.is_owner && (
              <Button size="sm" variant="secondary" onClick={() => void approve(e.id)}>
                {t('profile.public.approve')}
              </Button>
            )}
            {e.can_remove && (
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('lounge.profilePage.guestbookRemove', {
                  name: e.author ? nameOf(e.author) : t('profile.public.visitor'),
                })}
                onClick={() => void remove(e.id)}
              >
                {t('lounge.profilePage.remove')}
              </Button>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}

export const appSlots: BlockSlots = {
  now_watching: ({ page }) => <NowWatching page={page as ProfilePage} />,
  now_playing: ({ page }) => <NowPlaying page={page as ProfilePage} />,
  guestbook: ({ page, block }) => <Guestbook page={page as ProfilePage} block={block as Block} />,
};
