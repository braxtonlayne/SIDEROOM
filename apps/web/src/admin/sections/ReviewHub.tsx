// Review Queues hub (ADM-CNT-001): one tab per queue the staff member may work. Library submissions are the
// full reviewer workspace; Cinema is the report queue; Apps is developer applications and versions.
// Directory is the Server Directory queue; Packages has no server routes yet, so its tab says so.
import { EmptyState, Tabs, type TabItem } from '@sideroom/ui';
import { useLocation } from '@tanstack/react-router';
import { useT } from '../../i18n';
import { SectionHeader, useAdminViewer } from '../ui';
import { AppsReview } from './AppsReview';
import { DirectoryQueue } from './DirectoryQueue';
import { CinemaReports } from './CinemaReports';
import { LibraryReviewPanel } from './Review';

export function ReviewHubSection() {
  const t = useT();
  const viewer = useAdminViewer();
  // A `#review-queue` card links to `?submission=<id>` (G-68): that is a Library submission, so open its tab.
  const linked = /(^|&)submission=\d+/.test(
    useLocation({ select: (l) => l.searchStr }).replace(/^\?/, ''),
  );
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
      content: (
        <EmptyState
          title={t('admin.reviewHub.packagesTitle')}
          description={t('admin.reviewHub.notBuilt')}
        />
      ),
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
      <Tabs
        label={t('admin.reviewHub.tabsLabel')}
        items={items}
        {...(linked && items.some((i) => i.id === 'library') ? { defaultValue: 'library' } : {})}
      />
    </>
  );
}
