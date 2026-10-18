// `/u/:username` (EXT-PRF-002): a person's customised profile page, drawn by the renderer in a scoped
// container. A person who has not customised their page gets a plain card, not an empty grid.
import { EmptyState, Spinner, ThemeProvider } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useT } from '../i18n';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { cdnUrl } from '../store/cdn';
import { useLoad } from '../packages/useLoad';
import { getPageByName } from './api';
import { ProfileRenderer } from './ProfileRenderer';
import type { ProfilePage } from './types';
import './profilepage.css';

export function ProfilePageView({ username }: { username: string }) {
  const t = useT();
  const theme = useAppliedTheme();
  const res = useLoad<ProfilePage>(`page:${username}`, () => getPageByName(username));
  const page = res.data;
  return (
    <ThemeProvider
      section="lounge"
      userGlobal={theme.userGlobal}
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={theme.style}
      {...theme.data}
      className="pp-page"
    >
      <header className="pp-bar">
        <h1>{t('lounge.profilePage.title', { username })}</h1>
        <span className="pp-actions">
          <Link to="/channels" className="us-link">
            {t('lounge.profilePage.back')}
          </Link>{' '}
          {page?.is_owner && (
            <Link to="/profile-page/edit" className="us-link">
              {t('lounge.profilePage.edit')}
            </Link>
          )}
        </span>
      </header>
      <main className="pp-body">
        {res.loading && !page ? (
          <p className="pp-plain" role="status">
            <Spinner size="sm" label={t('lounge.profilePage.loading')} />{' '}
            {t('lounge.profilePage.loading')}
          </p>
        ) : res.error || !page ? (
          <div className="pp-plain">
            <EmptyState
              title={t('lounge.profilePage.notFoundTitle')}
              description={t('lounge.profilePage.notFoundBody')}
            />
          </div>
        ) : page.customised ? (
          <ProfileRenderer
            page={page}
            bannerUrl={cdnUrl('avatars', page.user.id, page.user.banner, { size: 1024 })}
          />
        ) : (
          <div className="pp-plain">
            <EmptyState
              title={t('lounge.profilePage.plainTitle', {
                name: page.user.display_name?.trim() || page.user.username,
              })}
              description={
                page.is_owner
                  ? t('lounge.profilePage.plainOwner')
                  : t('lounge.profilePage.plainViewer')
              }
            />
          </div>
        )}
      </main>
    </ThemeProvider>
  );
}
