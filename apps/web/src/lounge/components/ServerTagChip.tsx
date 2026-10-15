// The server tag next to a name (LNG-SSET-012): badge glyph plus 2 to 4 letters in the server's colour.
// Callers pass any LoungeUser; nothing is drawn unless the API sent a tag (it does so only while the member may
// display it).
import { useT } from '../../i18n';
import { badgeGlyph, hexOfColor } from '../../lib/serverTag';
import type { LoungeUser } from '../data';

export function ServerTagChip({
  user,
}: {
  user: Pick<LoungeUser, 'tag' | 'tagBadge' | 'tagColor'>;
}) {
  const t = useT();
  if (!user.tag) return null;
  const color = user.tagColor ? hexOfColor(user.tagColor) : undefined;
  return (
    <span className="lng-servertag" title={t('lounge.serverTag.title', { tag: user.tag })}>
      <span aria-hidden="true" {...(color ? { style: { color } } : {})}>
        {badgeGlyph(user.tagBadge ?? 0)}
      </span>{' '}
      {user.tag}
    </span>
  );
}
