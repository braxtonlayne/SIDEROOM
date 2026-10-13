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

function Img({
  src,
  alt,
  className,
}: {
  src: string | undefined;
  alt: string;
  className?: string;
}) {
  if (!isHttpUrl(src)) return null;
  return (
    <img src={src} alt={alt} className={className} loading="lazy" referrerPolicy="no-referrer" />
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
 * One embed card (LNG-MSGUI-016). Video embeds from other sites load only after a click, with a privacy
 * notice (LNG-MSGUI-017); remote images are fetched without a referrer.
 */
export function EmbedView({ embed: e, resolvers }: { embed: Embed; resolvers?: Resolvers }) {
  const [loaded, setLoaded] = useState(false);
  const color = colorHex(e.color);
  const isVideo = isHttpUrl(e.video?.url);
  const provider = e.provider?.name ?? (e.url ? hostOf(e.url) : 'a third-party site');

  if (e.type === 'image' && !e.title && !e.description) {
    return <Img src={e.thumbnail?.url ?? e.url} alt="" className="lng-embed-bare" />;
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
            <Img src={e.author.icon_url} alt="" className="lng-embed-author-icon" />
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
              <Img src={e.thumbnail?.url ?? e.image?.url} alt="" className="lng-embed-image" />
              <p className="lng-muted">{THIRD_PARTY_HINT}</p>
              <button type="button" className="lng-link-btn" onClick={() => setLoaded(true)}>
                Load video from {provider}
              </button>
            </div>
          ))}
        {!isVideo && <Img src={e.image?.url} alt="" className="lng-embed-image" />}
        {(e.footer || e.timestamp) && (
          <footer className="lng-embed-footer">
            <Img src={e.footer?.icon_url} alt="" className="lng-embed-footer-icon" />
            {e.footer?.text}
            {e.footer && e.timestamp && ' • '}
            {e.timestamp && <time dateTime={e.timestamp}>{formatFullTime(e.timestamp)}</time>}
          </footer>
        )}
      </div>
      {!isVideo && e.thumbnail && <Img src={e.thumbnail.url} alt="" className="lng-embed-thumb" />}
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
