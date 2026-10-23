// Review Queues hub (ADM-CNT-001): one tab per queue the staff member may work. Library submissions are the
// full reviewer workspace; Cinema is the report queue; Apps is developer applications and versions.
// Directory is the Server Directory queue; Packages is the declarative package queue (EXT-PKG-003).
import { Notice, Tabs, type TabItem } from '@sideroom/ui';
import { useLocation } from '@tanstack/react-router';
import { useT } from '../../i18n';
import * as moderation from '../../moderation/api';
import { SectionHeader, useAdminViewer, useResource } from '../ui';
import { AppsReview } from './AppsReview';
import { DirectoryQueue } from './DirectoryQueue';
import { CinemaReports } from './CinemaReports';
import { LibraryReviewPanel } from './Review';
import { PackagesReview } from './PackagesReview';

export function ReviewHubSection() {
  const t = useT();
  const viewer = useAdminViewer();
  // Open evasion signals (P-15), for staff who may see sign-in data; a failed count just hides the tile.
  const signalCount = useResource(`review-signals:${viewer.can('MANAGE_ACCOUNTS')}`, async () =>
    viewer.can('MANAGE_ACCOUNTS') ? (await moderation.countEvasionSignals()).open : null,
  );
  // Unattached reports (D-339): reports nobody has picked up, for `MANAGE_CASES`; a failed count hides the tile.
  const reportCount = useResource(`review-reports:${viewer.can('MANAGE_CASES')}`, async () =>
    viewer.can('MANAGE_CASES') ? (await moderation.getInboxCount()).reports : null,
  );
  // A `#review-queue` card links to `?submission=<id>` (G-68): that is a Library submission, so open its tab.
  const search = useLocation({ select: (l) => l.searchStr }).replace(/^\?/, '');
  const linked = /(^|&)submission=\d+/.test(search);
  // A package card links to `?package=<version id>` (D-258): open the Packages tab.
  const linkedPackage = /(^|&)package=\d+/.test(search);
  const items: TabItem[] = [];
  if (viewer.can('DELETE_CONTENT'))
    items.push({ id: 'cinema', label: t('admin.reviewHub.tabCinema'), content: <CinemaReports /> });
  if (viewer.can('REVIEW_READING') || viewer.can('REVIEW_MEDIA'))
    items.push({
      id: 'library',
      label: t('admin.reviewHub.tabLibrary'),
      content: <LibraryReviewPanel />,
    });
  if (viewer.can('REVIEW_APPS'))
    items.push({ id: 'apps', label: t('admin.reviewHub.tabApps'), content: <AppsReview /> });
  if (viewer.can('REVIEW_PACKAGES'))
    items.push({
      id: 'packages',
      label: t('admin.reviewHub.tabPackages'),
      content: <PackagesReview />,
    });
  if (viewer.can('MANAGE_DIRECTORY'))
    items.push({
      id: 'directory',
      label: t('admin.reviewHub.tabDirectory'),
      content: <DirectoryQueue />,
    });
  return (
    <>
      <SectionHeader title={t('admin.review.title')} description={t('admin.review.description')} />
      {signalCount.data != null && signalCount.data > 0 && (
        <Notice variant="info" role="status">
          <a href="/admin/moderation?tab=signals">
            {t('admin.suspicion.hubTile', { n: signalCount.data })}
          </a>
        </Notice>
      )}
      {reportCount.data != null && reportCount.data > 0 && (
        <Notice variant="info" role="status">
          <a href="/admin/moderation?tab=dashboard">
            {t('admin.reviewHub.reportsTile', { n: reportCount.data })}
          </a>
        </Notice>
      )}
      <Tabs
        label={t('admin.reviewHub.tabsLabel')}
        items={items}
        {...(linked && items.some((i) => i.id === 'library')
          ? { defaultValue: 'library' }
          : linkedPackage && items.some((i) => i.id === 'packages')
            ? { defaultValue: 'packages' }
            : {})}
      />
    </>
  );
}
