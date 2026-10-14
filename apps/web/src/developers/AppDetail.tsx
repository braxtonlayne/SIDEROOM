import { Button, EmptyState, Tabs } from '@sideroom/ui';
import { Link, useNavigate } from '@tanstack/react-router';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { isUnavailable } from './api';
import { AppIcon, StateBadge } from './Applications';
import { useApp } from './hooks';
import { BotTab } from './tabs/Bot';
import { CommandsTab } from './tabs/Commands';
import { GeneralTab } from './tabs/General';
import { InstallTab } from './tabs/Install';
import { ReviewTab } from './tabs/Review';
import { parseTab, type AppTab } from './search';
import { OAuth2Tab } from './tabs/OAuth2';
import { LoadError, Loading, NotAvailable } from './ui';

export function AppDetailPage({ appId, tab }: { appId: string; tab: AppTab }) {
  const t = useT();
  const navigate = useNavigate();
  const q = useApp(appId);

  if (q.isPending) return <Loading what={t('developers.appDetail.theApplication')} />;
  if (q.error instanceof ApiError && q.error.status === 401)
    return (
      <EmptyState
        title={t('developers.appDetail.signInTitle')}
        action={
          <Button
            onClick={() =>
              void navigate({ to: '/login', search: { next: `/developers/applications/${appId}` } })
            }
          >
            {t('developers.appDetail.logIn')}
          </Button>
        }
      />
    );
  if (q.error instanceof ApiError && (q.error.status === 404 || q.error.status === 403))
    return (
      <EmptyState
        title={t('developers.appDetail.notFoundTitle')}
        description={t('developers.appDetail.notFoundDescription')}
        action={
          <Link to="/developers/applications" className="dev-link">
            {t('developers.appDetail.backToApplications')}
          </Link>
        }
      />
    );
  if (isUnavailable(q.error))
    return <NotAvailable>{t('developers.appDetail.notAvailable')}</NotAvailable>;
  if (q.error)
    return (
      <LoadError
        what={t('developers.appDetail.theApplication')}
        error={q.error}
        onRetry={() => void q.refetch()}
      />
    );

  const app = q.data;
  return (
    <>
      <p className="dev-crumb">
        <Link to="/developers/applications" className="dev-link">
          {t('developers.appDetail.crumbApplications')}
        </Link>{' '}
        / {app.name}
      </p>
      <div className="dev-head">
        <div className="dev-title-row">
          <AppIcon app={app} size={56} />
          <h1 className="dev-title">{app.name}</h1>
          <StateBadge state={app.release_state} />
        </div>
      </div>
      <Tabs
        label={t('developers.appDetail.tabsLabel')}
        value={tab}
        onValueChange={(t) =>
          void navigate({
            to: '/developers/applications/$appId',
            params: { appId },
            search: { tab: parseTab(t) },
            replace: true,
          })
        }
        items={[
          {
            id: 'general',
            label: t('developers.appDetail.tabGeneral'),
            content: <GeneralTab key={app.id} app={app} />,
          },
          {
            id: 'oauth2',
            label: t('developers.appDetail.tabOauth2'),
            content: <OAuth2Tab app={app} />,
          },
          { id: 'bot', label: t('developers.appDetail.tabBot'), content: <BotTab app={app} /> },
          {
            id: 'installation',
            label: t('developers.appDetail.tabInstallation'),
            content: <InstallTab app={app} />,
          },
          {
            id: 'commands',
            label: t('developers.appDetail.tabCommands'),
            content: <CommandsTab app={app} />,
          },
          {
            id: 'review',
            label: t('developers.appDetail.tabReview'),
            content: <ReviewTab app={app} />,
          },
        ]}
      />
    </>
  );
}
