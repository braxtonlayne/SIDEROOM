import type { LoungeUser } from './data';
import { badgeGlyph, hexOfColor } from '../lib/serverTag';

/**
 * The server tag chip next to a name (G-34, LNG-GLD-011): the tag text with its badge glyph in the tag's
 * colour, in the `TagPreview` look of Server Settings. Without a badge or colour (older payloads, mock
 * data) it falls back to the plain grey chip.
 */
export function ServerTag({ user }: { user: Pick<LoungeUser, 'tag' | 'tagBadge' | 'tagColor'> }) {
  if (!user.tag) return null;
  const styled = user.tagBadge !== undefined && user.tagColor !== undefined;
  return (
    <span
      className="lng-tag"
      {...(styled ? { style: { color: hexOfColor(user.tagColor ?? 0) } } : {})}
    >
      {styled && <span aria-hidden="true">{badgeGlyph(user.tagBadge ?? 0)} </span>}
      {user.tag.toUpperCase()}
    </span>
  );
}
