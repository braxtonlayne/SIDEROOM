import { Button, EmptyState, Tabs } from '@sideroom/ui';
import { Link, useNavigate } from '@tanstack/react-router';
import { ApiError } from '../api/client';
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
  const navigate = useNavigate();
  const q = useApp(appId);

  if (q.isPending) return <Loading what="the application" />;
  if (q.error instanceof ApiError && q.error.status === 401)
    return (
      <EmptyState
        title="Sign in to continue"
        action={
          <Button
            onClick={() =>
              void navigate({ to: '/login', search: { next: `/developers/applications/${appId}` } })
            }
          >
            Log in
          </Button>
        }
      />
    );
  if (q.error instanceof ApiError && (q.error.status === 404 || q.error.status === 403))
    return (
      <EmptyState
        title="Application not found"
        description="It may have been deleted, or it belongs to someone else."
        action={
          <Link to="/developers/applications" className="dev-link">
            Back to your applications
          </Link>
        }
      />
    );
  if (isUnavailable(q.error))
    return (
      <NotAvailable>The developer platform is not switched on for this server yet.</NotAvailable>
    );
  if (q.error)
    return <LoadError what="the application" error={q.error} onRetry={() => void q.refetch()} />;

  const app = q.data;
  return (
    <>
      <p className="dev-crumb">
        <Link to="/developers/applications" className="dev-link">
          Applications
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
        label="Application settings"
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
          { id: 'general', label: 'General', content: <GeneralTab key={app.id} app={app} /> },
          { id: 'oauth2', label: 'OAuth2', content: <OAuth2Tab app={app} /> },
          { id: 'bot', label: 'Bot', content: <BotTab app={app} /> },
          { id: 'installation', label: 'Installation', content: <InstallTab app={app} /> },
          { id: 'commands', label: 'Commands', content: <CommandsTab app={app} /> },
          { id: 'review', label: 'Review', content: <ReviewTab app={app} /> },
        ]}
      />
    </>
  );
}
