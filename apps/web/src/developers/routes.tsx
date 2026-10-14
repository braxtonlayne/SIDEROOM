import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { parseTab, validateAuthorizeSearch } from './search';

// Pages load on demand (XC-PERF-001); the section frame is its own chunk with a bundle budget entry.
const DevelopersRoot = lazyNamed(() => import('./DevelopersRoot'), 'DevelopersRoot');
const OverviewPage = lazyNamed(() => import('./Overview'), 'OverviewPage');
const ApplicationsPage = lazyNamed(() => import('./Applications'), 'ApplicationsPage');
const AppDetailPage = lazyNamed(() => import('./AppDetail'), 'AppDetailPage');
const DocsIndexPage = lazyNamed(() => import('./Docs'), 'DocsIndexPage');
const DocPageView = lazyNamed(() => import('./Docs'), 'DocPageView');
const AuthorizePage = lazyNamed(() => import('./Authorize'), 'AuthorizePage');
const AuthorizedPage = lazyNamed(() => import('./Authorize'), 'AuthorizedPage');

/**
 * Developer portal routes:
 *   /developers                           overview
 *   /developers/applications              list + New application
 *   /developers/applications/:appId?tab=  General | OAuth2 | Bot | Installation | Commands | Review
 *   /developers/docs[/:page]              documentation hub
 *   /oauth2/authorize                     consent screen (outside the portal chrome)
 *   /oauth2/authorized                    landing after an approval that has no redirect URI
 */
export function createDeveloperRoutes(root: AnyRoute) {
  const layout = createRoute({
    getParentRoute: () => root,
    path: '/developers',
    component: DevelopersRoot,
  });
  const child = <P extends string>(path: P, component: () => React.JSX.Element | null) =>
    createRoute({ getParentRoute: () => layout, path, component });

  const detail = createRoute({
    getParentRoute: () => layout,
    path: 'applications/$appId',
    validateSearch: (s: Record<string, unknown>): { tab?: ReturnType<typeof parseTab> } =>
      s.tab === undefined ? {} : { tab: parseTab(s.tab) },
    component: function AppDetailRoute() {
      const { appId } = detail.useParams();
      const { tab } = detail.useSearch();
      return <AppDetailPage key={appId} appId={appId} tab={tab ?? 'general'} />;
    },
  });
  const doc = createRoute({
    getParentRoute: () => layout,
    path: 'docs/$page',
    component: function DocRoute() {
      return <DocPageView page={doc.useParams().page} />;
    },
  });
  const authorize = createRoute({
    getParentRoute: () => root,
    path: '/oauth2/authorize',
    validateSearch: validateAuthorizeSearch,
    component: function AuthorizeRoute() {
      return <AuthorizePage search={authorize.useSearch()} />;
    },
  });

  const authorized = createRoute({
    getParentRoute: () => root,
    path: '/oauth2/authorized',
    component: AuthorizedPage,
  });

  return [
    layout.addChildren([
      child('/', OverviewPage),
      child('applications', ApplicationsPage),
      detail,
      child('docs', DocsIndexPage),
      doc,
    ]),
    authorize,
    authorized,
  ];
}
