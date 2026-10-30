// The pure blocks of a profile page (EXT-PRF-003, PRF-PRO-001): everything that renders from the served page
// alone. Each renders plain text and https links from the document; nothing in a document is ever interpreted
// as markup. The stateful ones (the guestbook, a presence-driven Now watching) are slots the host fills.
import { useState, type ReactNode } from 'react';
import { useEnv } from './context';
import type { BlockLike, PageLike, PageUserLike } from './types';
import { httpsUrl } from './url';

const text = (c: Record<string, unknown>, k: string): string => {
  const v = c[k];
  return typeof v === 'string' ? v : '';
};
const list = <T,>(c: Record<string, unknown>, k: string): T[] => {
  const v = c[k];
  return Array.isArray(v) ? (v as T[]) : [];
};

export const nameOf = (u: PageUserLike) => u.display_name?.trim() || u.username;

/** A link out of the page: new tab, no referrer. */
export function Out({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer nofollow" className="pp-link">
      {children}
    </a>
  );
}

/** A round picture, or the first letter when there is none. */
export function Face({ name, src }: { name: string; src: string | null }) {
  return (
    <span className="pp-initial" aria-hidden="true">
      {src ? <img src={src} alt="" loading="lazy" /> : name.slice(0, 1).toUpperCase()}
    </span>
  );
}

function Header({ page, block }: { page: PageLike; block: BlockLike }) {
  const { t, cdn } = useEnv();
  const u = page.user;
  const tagline = text(block.config, 'tagline');
  const banner = block.config.show_banner !== false ? cdn('avatars', u.id, u.banner, 1024) : null;
  return (
    <div className="pp-header">
      {banner && <img className="pp-banner" src={banner} alt="" />}
      <div className="pp-header-row">
        <Face name={nameOf(u)} src={cdn('avatars', u.id, u.avatar, 128)} />
        <div>
          <h2 className="pp-name">{nameOf(u)}</h2>
          <p className="pp-muted">{t('profile.blocks.handle', { username: u.username })}</p>
          {tagline && <p className="pp-tagline">{tagline}</p>}
        </div>
      </div>
    </div>
  );
}

function About({ block }: { block: BlockLike }) {
  const { t } = useEnv();
  const body = text(block.config, 'text');
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.aboutTitle')}</h3>
      {body ? (
        <p className="pp-text">{body}</p>
      ) : (
        <p className="pp-muted">{t('profile.blocks.aboutEmpty')}</p>
      )}
    </>
  );
}

/** `profileHref` makes each friend a link (the app); the public page shows names only. */
function TopFriends({
  page,
  profileHref,
}: {
  page: PageLike;
  profileHref?: (username: string) => string | null;
}) {
  const { t, cdn } = useEnv();
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.friendsTitle')}</h3>
      {page.friends.length === 0 ? (
        <p className="pp-muted">{t('profile.blocks.friendsEmpty')}</p>
      ) : (
        <ol className="pp-friends">
          {page.friends.map((f, i) => {
            const href = profileHref?.(f.username) ?? null;
            const inner = (
              <>
                <Face name={nameOf(f)} src={cdn('avatars', f.id, f.avatar, 64)} />
                <span>{nameOf(f)}</span>
              </>
            );
            return (
              <li key={f.id}>
                <span className="pp-rank" aria-hidden="true">
                  {i + 1}
                </span>
                {href ? (
                  <a className="pp-link" href={href}>
                    {inner}
                  </a>
                ) : (
                  <span className="pp-link">{inner}</span>
                )}
              </li>
            );
          })}
        </ol>
      )}
    </>
  );
}

function Favorites({ block }: { block: BlockLike }) {
  const { t } = useEnv();
  const items = list<string>(block.config, 'items').filter((s) => typeof s === 'string');
  return (
    <>
      <h3 className="pp-title">
        {text(block.config, 'title') || t('profile.blocks.favoritesTitle')}
      </h3>
      {items.length === 0 ? (
        <p className="pp-muted">{t('profile.blocks.favoritesEmpty')}</p>
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

/**
 * One gallery picture. A web address is loaded only through the signed image-proxy path the server filled in
 * (`proxy_url`, D-259, D-176), never from its own host; with no proxy path the tile shows the caption. An
 * uploaded picture comes from our own CDN (K-17).
 */
function GalleryTile({
  src,
  href,
  caption,
  n,
}: {
  src: string | null;
  href: string | null;
  caption: string;
  n: number;
}) {
  const { t } = useEnv();
  const [failed, setFailed] = useState(false);
  const label = caption || t('profile.blocks.galleryPicture', { n });
  return (
    <figure>
      {src && !failed ? (
        <img
          className="pp-pic"
          src={src}
          alt={caption}
          loading="lazy"
          referrerPolicy="no-referrer"
          onError={() => setFailed(true)}
        />
      ) : (
        <div className="pp-pic pp-pic-missing" role="img" aria-label={label}>
          <span>{label}</span>
        </div>
      )}
      <figcaption>{href ? <Out href={href}>{label}</Out> : label}</figcaption>
    </figure>
  );
}

interface GalleryItemLike {
  url?: string;
  image_id?: string;
  caption?: string;
  proxy_url?: string;
}

function Gallery({ block, page }: { block: BlockLike; page: PageLike }) {
  const { t } = useEnv();
  const media = new Map((page.media ?? []).map((m) => [m.id, m]));
  const items = list<GalleryItemLike>(block.config, 'items').flatMap(
    (it): { src: string | null; href: string | null; caption: string }[] => {
      const caption = it?.caption ?? '';
      if (it?.image_id) {
        const m = media.get(it.image_id);
        return m ? [{ src: m.url, href: null, caption }] : [];
      }
      const href = httpsUrl(it?.url);
      if (!href) return [];
      const proxy = typeof it.proxy_url === 'string' && it.proxy_url.startsWith('/cdn/external/');
      return [{ src: proxy ? (it.proxy_url ?? null) : null, href, caption }];
    },
  );
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.galleryTitle')}</h3>
      {items.length === 0 ? (
        <p className="pp-muted">{t('profile.blocks.galleryEmpty')}</p>
      ) : (
        <ul className="pp-gallery">
          {items.map((it, i) => (
            <li key={i}>
              <GalleryTile src={it.src} href={it.href} caption={it.caption} n={i + 1} />
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function Song({ block }: { block: BlockLike }) {
  const { t } = useEnv();
  const [asking, setAsking] = useState(false);
  const title = text(block.config, 'title');
  const artist = text(block.config, 'artist');
  const url = httpsUrl(text(block.config, 'url'));
  const host = url ? new URL(url).hostname : '';
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.songTitle')}</h3>
      {title ? (
        <p className="pp-text">
          <strong>{title}</strong>
          {artist && <span className="pp-muted"> {artist}</span>}
        </p>
      ) : (
        <p className="pp-muted">{t('profile.blocks.songEmpty')}</p>
      )}
      {url && !asking && (
        <button type="button" className="pp-link" onClick={() => setAsking(true)}>
          {t('profile.blocks.songPlay')}
        </button>
      )}
      {url && asking && (
        <p className="pp-text" role="status">
          {t('profile.blocks.songConfirm', { host })}{' '}
          <Out href={url}>{t('profile.blocks.songOpen')}</Out>{' '}
          <button type="button" className="pp-link" onClick={() => setAsking(false)}>
            {t('profile.blocks.cancel')}
          </button>
        </p>
      )}
    </>
  );
}

function Custom({ block }: { block: BlockLike }) {
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

/** PRF-PRO-001: a stack of links, each opened in a new tab without a referrer. */
function Linktree({ block }: { block: BlockLike }) {
  const { t } = useEnv();
  const links = list<{ label: string; url: string }>(block.config, 'links')
    .map((l) => ({ label: (l?.label ?? '').trim(), href: httpsUrl(l?.url) }))
    .filter((l): l is { label: string; href: string } => l.href !== null);
  return (
    <>
      <h3 className="pp-title">{text(block.config, 'title') || t('profile.blocks.linksTitle')}</h3>
      {links.length === 0 ? (
        <p className="pp-muted">{t('profile.blocks.linksEmpty')}</p>
      ) : (
        <ul className="pp-linktree">
          {links.map((l, i) => (
            <li key={i}>
              <a href={l.href} target="_blank" rel="noopener noreferrer nofollow">
                {l.label || l.href}
              </a>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** PRF-PRO-001: the servers the server-side filter already admitted for this viewer (members' and directory). */
function FeaturedGuilds({ page }: { page: PageLike }) {
  const { t, cdn } = useEnv();
  const guilds = page.featured_guilds ?? [];
  return (
    <>
      <h3 className="pp-title">{t('profile.blocks.serversTitle')}</h3>
      {guilds.length === 0 ? (
        <p className="pp-muted">{t('profile.blocks.serversEmpty')}</p>
      ) : (
        <ul className="pp-servers">
          {guilds.map((g) => (
            <li key={g.id}>
              <Face name={g.name} src={cdn('icons', g.id, g.icon, 64)} />
              <span>{g.name}</span>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** What the member typed by hand (`manual` mode). `auto` needs presence, which only the host has. */
function Typed({ block, title }: { block: BlockLike; title: string }) {
  const { t } = useEnv();
  const line = text(block.config, 'title');
  const subtitle = text(block.config, 'subtitle');
  return (
    <>
      <h3 className="pp-title">{title}</h3>
      {line ? (
        <p className="pp-text">
          <strong>{line}</strong>
          {subtitle && <span className="pp-muted"> {subtitle}</span>}
        </p>
      ) : (
        <p className="pp-muted">{t('profile.blocks.playingEmpty')}</p>
      )}
    </>
  );
}

/** What the host can take over: the blocks that need more than the served page. */
export interface BlockSlots {
  now_watching?: (ctx: { block: BlockLike; page: PageLike }) => ReactNode;
  now_playing?: (ctx: { block: BlockLike; page: PageLike }) => ReactNode;
  guestbook?: (ctx: { block: BlockLike; page: PageLike }) => ReactNode;
}

export function BlockView({
  block,
  page,
  compact = false,
  slots,
  profileHref,
}: {
  block: BlockLike;
  page: PageLike;
  compact?: boolean;
  slots?: BlockSlots;
  profileHref?: (username: string) => string | null;
}) {
  const { t } = useEnv();
  const manual = text(block.config, 'mode') === 'manual';
  switch (block.type) {
    case 'header':
      return <Header page={page} block={block} />;
    case 'about':
      return <About block={block} />;
    case 'top_friends':
      return <TopFriends page={page} profileHref={profileHref} />;
    case 'now_watching':
      if (slots?.now_watching && !manual) return <>{slots.now_watching({ block, page })}</>;
      return manual ? <Typed block={block} title={t('profile.blocks.nowTitle')} /> : null;
    case 'now_playing':
      if (slots?.now_playing && !manual) return <>{slots.now_playing({ block, page })}</>;
      return manual ? <Typed block={block} title={t('profile.blocks.playingTitle')} /> : null;
    case 'favorites':
      return <Favorites block={block} />;
    case 'gallery':
      return <Gallery block={block} page={page} />;
    case 'guestbook':
      return compact || !slots?.guestbook ? null : <>{slots.guestbook({ block, page })}</>;
    case 'song':
      return <Song block={block} />;
    case 'custom':
      return <Custom block={block} />;
    case 'linktree':
      return <Linktree block={block} />;
    case 'featured_guilds':
      return <FeaturedGuilds page={page} />;
    default:
      return null;
  }
}
