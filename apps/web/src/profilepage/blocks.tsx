// The blocks of a profile page (EXT-PRF-003): Header, About me, Top friends, Now watching/reading, Favorites,
// Gallery, Guestbook, Song and Custom box. Each one renders plain text and https links from the document;
// nothing in a document is ever interpreted as markup.
import { Avatar, Button, TextArea } from '@sideroom/ui';
import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../api/client';
import { formatDate, useT } from '../i18n';
import { usePresence } from '../lounge/data';
import { ActivityList } from '../lounge/profile/parts';
import { cdnUrl } from '../store/cdn';
import { listGuestbook, removeGuestbookEntry, signGuestbook } from './api';
import type { Block, GuestbookEntry, PageUser, ProfilePage } from './types';

const text = (c: Record<string, unknown>, k: string): string => {
  const v = c[k];
  return typeof v === 'string' ? v : '';
};
const list = <T,>(c: Record<string, unknown>, k: string): T[] => {
  const v = c[k];
  return Array.isArray(v) ? (v as T[]) : [];
};

const nameOf = (u: PageUser) => u.display_name?.trim() || u.username;
const avatarOf = (u: PageUser, size = 64) => cdnUrl('avatars', u.id, u.avatar, { size });

/** Only https links are followed from a profile page. */
export const httpsUrl = (u: string | null | undefined): string | null => {
  if (!u) return null;
  try {
    const url = new URL(u);
    return url.protocol === 'https:' && !url.username && !url.password ? url.toString() : null;
  } catch {
    return null;
  }
};

/** A link out of the app: new tab, no referrer. */
function Out({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer nofollow" className="pp-link">
      {children}
    </a>
  );
}

function Header({ page, block }: { page: ProfilePage; block: Block }) {
  const t = useT();
  const u = page.user;
  const tagline = text(block.config, 'tagline');
  const showBanner = block.config.show_banner !== false;
  const banner = showBanner ? cdnUrl('avatars', u.id, u.banner, { size: 1024 }) : null;
  return (
    <div className="pp-header">
      {banner && <img className="pp-banner" src={banner} alt="" />}
      <div className="pp-header-row">
        <Avatar name={nameOf(u)} src={avatarOf(u, 128)} size={80} />
        <div>
          <h2 className="pp-name">{nameOf(u)}</h2>
          <p className="pp-muted">{t('lounge.profilePage.handle', { username: u.username })}</p>
          {tagline && <p className="pp-tagline">{tagline}</p>}
        </div>
      </div>
    </div>
  );
}

function About({ block }: { block: Block }) {
  const t = useT();
  const body = text(block.config, 'text');
  return (
    <>
      <h3 className="pp-title">{t('lounge.profilePage.aboutTitle')}</h3>
      {body ? (
        <p className="pp-text">{body}</p>
      ) : (
        <p className="pp-muted">{t('lounge.profilePage.aboutEmpty')}</p>
      )}
    </>
  );
}

function TopFriends({ page }: { page: ProfilePage }) {
  const t = useT();
  return (
    <>
      <h3 className="pp-title">{t('lounge.profilePage.friendsTitle')}</h3>
      {page.friends.length === 0 ? (
        <p className="pp-muted">{t('lounge.profilePage.friendsEmpty')}</p>
      ) : (
        <ol className="pp-friends">
          {page.friends.map((f, i) => (
            <li key={f.id}>
              <span className="pp-rank" aria-hidden="true">
                {i + 1}
              </span>
              <a className="pp-link" href={`/u/${encodeURIComponent(f.username)}`}>
                <Avatar name={nameOf(f)} src={avatarOf(f)} size={40} />
                <span>{nameOf(f)}</span>
              </a>
            </li>
          ))}
        </ol>
      )}
    </>
  );
}

function NowWatching({ page, block }: { page: ProfilePage; block: Block }) {
  const t = useT();
  const presence = usePresence(page.user.id);
  const manual = text(block.config, 'mode') === 'manual';
  const title = text(block.config, 'title');
  const subtitle = text(block.config, 'subtitle');
  const now = (presence.activities ?? []).filter(
    (a) => a.kind === 'watching' || a.kind === 'reading',
  );
  return (
    <>
      <h3 className="pp-title">{t('lounge.profilePage.nowTitle')}</h3>
      {manual ? (
        title ? (
          <p className="pp-text">
            <strong>{title}</strong>
            {subtitle && <span className="pp-muted"> {subtitle}</span>}
          </p>
        ) : (
          <p className="pp-muted">{t('lounge.profilePage.nowEmpty')}</p>
        )
      ) : now.length > 0 ? (
        <ActivityList activities={now} />
      ) : (
        <p className="pp-muted">{t('lounge.profilePage.nowEmpty')}</p>
      )}
    </>
  );
}

function Favorites({ block }: { block: Block }) {
  const t = useT();
  const items = list<string>(block.config, 'items').filter((s) => typeof s === 'string');
  return (
    <>
      <h3 className="pp-title">
        {text(block.config, 'title') || t('lounge.profilePage.favoritesTitle')}
      </h3>
      {items.length === 0 ? (
        <p className="pp-muted">{t('lounge.profilePage.favoritesEmpty')}</p>
      ) : (
        <ol className="pp-list">
          {items.map((it, i) => (
            <li key={i}>{it}</li>
          ))}
        </ol>
      )}
    </>
  );
}

function Gallery({ block }: { block: Block }) {
  const t = useT();
  const items = list<{ url: string; caption?: string }>(block.config, 'items')
    .map((it) => ({ href: httpsUrl(it?.url), caption: it?.caption ?? '' }))
    .filter((it): it is { href: string; caption: string } => it.href !== null);
  return (
    <>
      <h3 className="pp-title">{t('lounge.profilePage.galleryTitle')}</h3>
      {items.length === 0 ? (
        <p className="pp-muted">{t('lounge.profilePage.galleryEmpty')}</p>
      ) : (
        // Pictures are links, not embeds: loading them would tell their host who looks at this page.
        <ul className="pp-gallery">
          {items.map((it, i) => (
            <li key={i}>
              <Out href={it.href}>
                {it.caption || t('lounge.profilePage.galleryPicture', { n: i + 1 })}
              </Out>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function Song({ block }: { block: Block }) {
  const t = useT();
  const [asking, setAsking] = useState(false);
  const title = text(block.config, 'title');
  const artist = text(block.config, 'artist');
  const url = httpsUrl(text(block.config, 'url'));
  const host = url ? new URL(url).hostname : '';
  return (
    <>
      <h3 className="pp-title">{t('lounge.profilePage.songTitle')}</h3>
      {title ? (
        <p className="pp-text">
          <strong>{title}</strong>
          {artist && <span className="pp-muted"> {artist}</span>}
        </p>
      ) : (
        <p className="pp-muted">{t('lounge.profilePage.songEmpty')}</p>
      )}
      {url && !asking && (
        <Button size="sm" variant="secondary" onClick={() => setAsking(true)}>
          {t('lounge.profilePage.songPlay')}
        </Button>
      )}
      {url && asking && (
        <p className="pp-text" role="status">
          {t('lounge.profilePage.songConfirm', { host })}{' '}
          <Out href={url}>{t('lounge.profilePage.songOpen')}</Out>{' '}
          <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
            {t('lounge.profilePage.cancel')}
          </Button>
        </p>
      )}
    </>
  );
}

function Custom({ block }: { block: Block }) {
  const body = text(block.config, 'text');
  const title = text(block.config, 'title');
  const links = list<{ label: string; url: string }>(block.config, 'links')
    .map((l) => ({ label: l?.label ?? '', href: httpsUrl(l?.url) }))
    .filter((l): l is { label: string; href: string } => l.href !== null);
  return (
    <>
      {title && <h3 className="pp-title">{title}</h3>}
      {body && <p className="pp-text">{body}</p>}
      {links.length > 0 && (
        <ul className="pp-list">
          {links.map((l, i) => (
            <li key={i}>
              <Out href={l.href}>{l.label || l.href}</Out>
            </li>
          ))}
        </ul>
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
            <Avatar name={nameOf(e.author)} src={avatarOf(e.author)} size={32} />
            <div>
              <p className="pp-muted">
                <a className="pp-link" href={`/u/${encodeURIComponent(e.author.username)}`}>
                  {nameOf(e.author)}
                </a>{' '}
                {formatDate(new Date(e.created_at_ms), { dateStyle: 'medium' })}
              </p>
              <p className="pp-text">{e.body}</p>
            </div>
            {e.can_remove && (
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('lounge.profilePage.guestbookRemove', { name: nameOf(e.author) })}
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

export function BlockView({
  block,
  page,
  compact,
}: {
  block: Block;
  page: ProfilePage;
  compact: boolean;
}) {
  switch (block.type) {
    case 'header':
      return <Header page={page} block={block} />;
    case 'about':
      return <About block={block} />;
    case 'top_friends':
      return <TopFriends page={page} />;
    case 'now_watching':
      return <NowWatching page={page} block={block} />;
    case 'favorites':
      return <Favorites block={block} />;
    case 'gallery':
      return <Gallery block={block} />;
    case 'guestbook':
      return compact ? null : <Guestbook page={page} block={block} />;
    case 'song':
      return <Song block={block} />;
    case 'custom':
      return <Custom block={block} />;
    default:
      return null;
  }
}
