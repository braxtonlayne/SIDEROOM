import type { Resolvers } from '@sideroom/markdown-react';
import { useState, type CSSProperties } from 'react';
import type { Embed } from '../data';
import { formatFullTime } from '../time';
import { isHttpUrl } from './format';
import { Md } from './Md';

const THIRD_PARTY_HINT =
  'Loading this video connects to a third-party site, which can see your IP address.';

function colorHex(c: number | undefined): string | undefined {
  return c === undefined ? undefined : `#${(c & 0xffffff).toString(16).padStart(6, '0')}`;
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return 'a third-party site';
  }
}

/** The image proxy's path (D-176). Only this exact same-origin form loads without a click. */
const PROXY_PREFIX = '/cdn/external/';

function isProxyUrl(u: string | undefined): u is string {
  return typeof u === 'string' && u.startsWith(PROXY_PREFIX) && !u.includes('\\');
}

/**
 * The URL an embed image may be loaded from right now. A `proxy_url` from our own server is always
 * fine. A third-party `url` is never put into an <img> until the reader asked for it ("Load image").
 */
function pick(
  url: string | undefined,
  proxy: string | undefined,
  loaded: boolean,
): string | undefined {
  if (isProxyUrl(proxy)) return proxy;
  return loaded && isHttpUrl(url) ? url : undefined;
}

/** The original host of every third-party image of this embed that has no proxy copy. */
function remoteImageHosts(e: Embed): string[] {
  const candidates: [string | undefined, string | undefined][] = [
    [e.image?.url, e.image?.proxy_url],
    [e.thumbnail?.url, e.thumbnail?.proxy_url],
    [e.author?.icon_url, e.author?.proxy_icon_url],
    [e.footer?.icon_url, e.footer?.proxy_icon_url],
    // A video embed shows its still image behind the video gate.
  ];
  const hosts = new Set<string>();
  for (const [url, proxy] of candidates) {
    if (isHttpUrl(url) && !isProxyUrl(proxy)) hosts.add(hostOf(url));
  }
  return [...hosts];
}

function Img({
  src,
  alt,
  className,
}: {
  src: string | undefined;
  alt: string;
  className?: string;
}) {
  if (!src) return null;
  return (
    <img
      src={src}
      alt={alt}
      className={className}
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
    />
  );
}

/** One control per embed: reveals every image of the embed that is still on the other site. */
function LoadImages({ hosts, onLoad }: { hosts: string[]; onLoad: () => void }) {
  if (hosts.length === 0) return null;
  const label = hosts.join(', ');
  return (
    <div className="lng-embed-gate">
      <p className="lng-muted">
        {hosts.length === 1 ? 'This image is' : 'These images are'} hosted on {label}. Loading{' '}
        {hosts.length === 1 ? 'it' : 'them'} shares your IP address with{' '}
        {hosts.length === 1 ? 'that site' : 'those sites'}.
      </p>
      <button
        type="button"
        className="lng-link-btn"
        aria-label={`Load image from ${label}`}
        onClick={onLoad}
      >
        Load image
      </button>
    </div>
  );
}

function Link({ href, children }: { href: string | undefined; children: string }) {
  return isHttpUrl(href) ? (
    <a href={href} target="_blank" rel="noreferrer noopener">
      {children}
    </a>
  ) : (
    <>{children}</>
  );
}

/**
 * One embed card (LNG-MSGUI-016). Text always renders. An image is shown straight away only through its
 * `proxy_url` (LNG-MSG-014, D-176); an image still on the other site waits behind one "Load image"
 * control per embed and then loads without a referrer. Video embeds from other sites load only after a
 * click, with a privacy notice (LNG-MSGUI-017).
 */
export function EmbedView({ embed: e, resolvers }: { embed: Embed; resolvers?: Resolvers }) {
  const [loaded, setLoaded] = useState(false);
  const [imagesLoaded, setImagesLoaded] = useState(false);
  const color = colorHex(e.color);
  const isVideo = isHttpUrl(e.video?.url);
  const provider = e.provider?.name ?? (e.url ? hostOf(e.url) : 'a third-party site');

  const hosts = imagesLoaded ? [] : remoteImageHosts(e);
  const imageSrc = pick(e.image?.url, e.image?.proxy_url, imagesLoaded);
  const thumbSrc = pick(e.thumbnail?.url, e.thumbnail?.proxy_url, imagesLoaded);

  if (e.type === 'image' && !e.title && !e.description) {
    // A bare image link: the picture is the whole message. Its source is the thumbnail, or the image.
    const bare = e.thumbnail ? thumbSrc : imageSrc;
    return (
      <>
        <Img src={bare} alt="" className="lng-embed-bare" />
        <LoadImages hosts={hosts} onLoad={() => setImagesLoaded(true)} />
      </>
    );
  }
  return (
    <article
      className="lng-embed"
      style={color ? ({ '--embed-color': color } as CSSProperties) : undefined}
      aria-label={e.title ? `Embed: ${e.title}` : 'Embed'}
    >
      <div className="lng-embed-main">
        {e.provider?.name && <div className="lng-embed-provider">{e.provider.name}</div>}
        {e.author && (
          <div className="lng-embed-author">
            <Img
              src={pick(e.author.icon_url, e.author.proxy_icon_url, imagesLoaded)}
              alt=""
              className="lng-embed-author-icon"
            />
            <Link href={e.author.url}>{e.author.name}</Link>
          </div>
        )}
        {e.title && (
          <div className="lng-embed-title">
            <Link href={e.url}>{e.title}</Link>
          </div>
        )}
        {e.description && (
          <div className="lng-embed-desc">
            <Md text={e.description} resolvers={resolvers} />
          </div>
        )}
        {e.fields && e.fields.length > 0 && (
          <div className="lng-embed-fields">
            {e.fields.map((f, i) => (
              <div key={i} className="lng-embed-field" data-inline={f.inline || undefined}>
                <div className="lng-embed-field-name">{f.name}</div>
                <div>
                  <Md text={f.value} resolvers={resolvers} />
                </div>
              </div>
            ))}
          </div>
        )}
        {isVideo &&
          (loaded ? (
            <iframe
              className="lng-embed-frame"
              title={e.title ?? `Video from ${provider}`}
              src={e.video!.url}
              allow="fullscreen; encrypted-media"
              sandbox="allow-scripts allow-same-origin allow-presentation allow-popups"
              referrerPolicy="no-referrer"
              allowFullScreen
            />
          ) : (
            <div className="lng-embed-gate">
              <Img src={thumbSrc ?? imageSrc} alt="" className="lng-embed-image" />
              <p className="lng-muted">{THIRD_PARTY_HINT}</p>
              <button type="button" className="lng-link-btn" onClick={() => setLoaded(true)}>
                Load video from {provider}
              </button>
            </div>
          ))}
        {!isVideo && <Img src={imageSrc} alt="" className="lng-embed-image" />}
        <LoadImages hosts={hosts} onLoad={() => setImagesLoaded(true)} />
        {(e.footer || e.timestamp) && (
          <footer className="lng-embed-footer">
            <Img
              src={pick(e.footer?.icon_url, e.footer?.proxy_icon_url, imagesLoaded)}
              alt=""
              className="lng-embed-footer-icon"
            />
            {e.footer?.text}
            {e.footer && e.timestamp && ' • '}
            {e.timestamp && <time dateTime={e.timestamp}>{formatFullTime(e.timestamp)}</time>}
          </footer>
        )}
      </div>
      {!isVideo && e.thumbnail && <Img src={thumbSrc} alt="" className="lng-embed-thumb" />}
    </article>
  );
}

export function EmbedList({ embeds, resolvers }: { embeds: Embed[]; resolvers?: Resolvers }) {
  return (
    <div className="lng-embeds">
      {embeds.slice(0, 10).map((e, i) => (
        <EmbedView key={i} embed={e} {...(resolvers ? { resolvers } : {})} />
      ))}
    </div>
  );
}
