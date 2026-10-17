// The profile page renderer (EXT-PRF-002, EXT-PRF-006). A person's document becomes a scoped container: every
// colour and choice is set as a custom property or data attribute on the container itself and nothing leaks
// to the rest of the app. The document is data (D-063); this component maps it to markup and never renders
// anything the document supplies as HTML.
import type { CSSProperties } from 'react';
import { BlockView } from './blocks';
import { GRID_COLUMNS, type Block, type ProfileDoc, type ProfilePage } from './types';
import './profilepage.css';

const FONT_STACK: Record<string, string> = {
  inter: "'Inter', system-ui, sans-serif",
  newsreader: "'Newsreader', Georgia, serif",
  'hanken-grotesk': "'Hanken Grotesk', system-ui, sans-serif",
  'plex-mono': "'IBM Plex Mono', ui-monospace, monospace",
};

const HEX = /^#[0-9a-fA-F]{6}$/;
const safeHex = (v: string, fallback: string) => (HEX.test(v) ? v : fallback);

/** The custom properties of a document's style. Exported for the editor's live preview. */
export function pageVars(doc: ProfileDoc): CSSProperties {
  const p = doc.style.palette;
  return {
    '--pp-bg': safeHex(p.background, '#1e1f22'),
    '--pp-panel': safeHex(p.panel, '#2b2d31'),
    '--pp-text': safeHex(p.text, '#f2f3f5'),
    '--pp-accent': safeHex(p.accent, '#5865f2'),
    '--pp-font': FONT_STACK[doc.style.font] ?? FONT_STACK.inter,
  } as CSSProperties;
}

/** Block order for the compact (single column) layouts: top to bottom, then left to right. */
export const reading = (blocks: readonly Block[]): Block[] =>
  [...blocks].sort((a, b) => a.y - b.y || a.x - b.x);

export function ProfileRenderer({
  page,
  doc = page.doc,
  compact = false,
  bannerUrl = null,
}: {
  page: ProfilePage;
  /** The document to draw; the editor passes its draft. */
  doc?: ProfileDoc;
  /** The mini popout: one column, no guestbook, at most five blocks. */
  compact?: boolean;
  /** The owner's banner image, used when the style asks for it as the background. */
  bannerUrl?: string | null;
}) {
  const blocks = reading(doc.blocks).filter((b) => !compact || b.type !== 'guestbook');
  const shown = compact ? blocks.slice(0, 5) : blocks;
  return (
    <div
      className="pp-root"
      data-compact={compact || undefined}
      data-font={doc.style.font}
      data-cursor={doc.style.cursor}
      data-overlay={doc.style.overlay}
      data-border={doc.style.border}
      data-pattern={doc.style.pattern}
      data-banner-bg={doc.style.banner_background && bannerUrl ? 'true' : undefined}
      style={{
        ...pageVars(doc),
        ...(doc.style.banner_background && bannerUrl
          ? ({ '--pp-banner': `url("${bannerUrl}")` } as CSSProperties)
          : {}),
      }}
    >
      {doc.style.overlay !== 'none' && <span className="pp-overlay" aria-hidden="true" />}
      <div
        className="pp-grid"
        style={compact ? undefined : { gridTemplateColumns: `repeat(${GRID_COLUMNS}, 1fr)` }}
      >
        {shown.map((b) => (
          <section
            key={b.id}
            className="pp-block"
            data-type={b.type}
            style={
              compact
                ? undefined
                : { gridColumn: `${b.x + 1} / span ${b.w}`, gridRow: `${b.y + 1} / span ${b.h}` }
            }
          >
            <BlockView block={b} page={page} compact={compact} />
          </section>
        ))}
      </div>
    </div>
  );
}
