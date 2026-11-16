// The profile page renderer in the app (EXT-PRF-002, EXT-PRF-006). The renderer itself is the shared package
// @sideroom/profile-blocks (the public profile surface uses the same one, D-385); this wrapper gives it the app's
// translator, image addresses, the QR drawer for Bitcoin and Lightning tip links (FU-18; not in the compact popout)
// and the stateful blocks (guestbook form, presence).
import {
  BlocksEnv,
  ProfileRenderer as Renderer,
  pageVars,
  reading,
  type BlockSlots,
} from '@sideroom/profile-blocks';
import { useT } from '../i18n';
import { cdnUrl } from '../store/cdn';
import { QrBox } from '../tips/QrBox';
import { HeaderPicture, appSlots } from './blocks';
import type { ProfileDoc, ProfileMedia, ProfilePage } from './types';

/** The popout's header picture is the 64 px one, in the same ring (D-504). */
const compactSlots: BlockSlots = {
  ...appSlots,
  avatar: ({ page }) => <HeaderPicture page={page as ProfilePage} size={64} />,
};

export { pageVars, reading };

const cdn = (
  kind: 'avatars' | 'icons',
  owner: string,
  hash: string | null | undefined,
  size: number,
) => cdnUrl(kind, owner, hash, { size });

const profileHref = (username: string) => `/u/${encodeURIComponent(username)}`;

export function ProfileRenderer({
  page,
  doc = page.doc,
  compact = false,
  bannerUrl = null,
  media = page.media ?? [],
}: {
  page: ProfilePage;
  /** The document to draw; the editor passes its draft. */
  doc?: ProfileDoc;
  /** The mini popout: one column, no guestbook, at most five blocks. */
  compact?: boolean;
  /** The owner's banner image, used when the style asks for it as the background. */
  bannerUrl?: string | null;
  /** The owner's uploaded pictures, for a picture background (the editor passes its own list). */
  media?: ProfileMedia[];
}) {
  const t = useT();
  return (
    <BlocksEnv t={t} cdn={cdn} tipQr={compact ? undefined : QrBox}>
      <Renderer
        page={page}
        doc={doc}
        compact={compact}
        bannerUrl={bannerUrl}
        media={media}
        slots={compact ? compactSlots : appSlots}
        profileHref={profileHref}
      />
    </BlocksEnv>
  );
}
