// The compact rendering of a customised profile for the profile popout (EXT-PRF-006): the same document in one
// column, at most five blocks, no guestbook, and a link to the whole page. Renders nothing for a person who
// has not customised their page or whose page cannot be read.
import { useT } from '../i18n';
import { useLoad } from '../packages/useLoad';
import { cdnUrl } from '../store/cdn';
import { getPageById } from './api';
import { ProfileRenderer } from './ProfileRenderer';
import type { ProfilePage } from './types';

export function MiniProfile({ userId }: { userId: string }) {
  const t = useT();
  const res = useLoad<ProfilePage | null>(`mini:${userId}`, () =>
    getPageById(userId).catch(() => null),
  );
  const page = res.data;
  if (!page || !page.customised) return null;
  return (
    <section aria-labelledby="pop-page-h">
      <h3 id="pop-page-h" className="lng-pop-h">
        {t('lounge.profilePage.miniHeading')}
      </h3>
      <ProfileRenderer
        page={page}
        compact
        bannerUrl={cdnUrl('avatars', page.user.id, page.user.banner, { size: 512 })}
      />
      <p>
        <a className="lng-link-btn" href={`/u/${encodeURIComponent(page.user.username)}`}>
          {t('lounge.profilePage.miniOpen')}
        </a>
      </p>
    </section>
  );
}
