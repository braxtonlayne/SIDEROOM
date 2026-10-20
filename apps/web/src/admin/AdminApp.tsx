/**
 * Platform admin shell (ADM-UI-001). Route: /admin/:section.
 * Only sections the staff member's platform permissions allow are listed; the API enforces the same rules,
 * so a hidden section is a courtesy, never the security boundary.
 */
import {
  Button,
  EmptyState,
  SettingsLayout,
  ThemeProvider,
  ToastProvider,
  type SettingsSection,
} from '@sideroom/ui';
import { Navigate, useNavigate, useParams } from '@tanstack/react-router';
import { useEffect, useMemo, useState, type ComponentType } from 'react';
import { ApiError } from '../api/client';
import { t as translate, useT } from '../i18n';
import * as api from './api';
import { hasAny, parsePerms } from './permissions';
import { ReauthHost } from './ReauthHost';
import { useStaffRevoked } from './useStaffRevoked';
import { AccessLogSection } from './sections/AccessLog';
import { AnnouncementsSection } from './sections/Announcements';
import { AuditLogSection } from './sections/AuditLog';
import { CatalogSection } from './cinema/CatalogSection';
import { LibraryCatalogSection } from './library/LibraryCatalogSection';
import { IngestSourcesSection } from './sections/IngestSources';
import { CollectionsSection } from './cinema/CollectionsSection';
import { TranscodingSection } from './cinema/TranscodingSection';
import { LiveTvSection } from './cinema/tv/LiveTvSection';
import { EntitlementsSection } from './donations/EntitlementsSection';
import { InvoicesSection } from './donations/InvoicesSection';
import { DonationsOverviewSection } from './donations/OverviewSection';
import { PricingSection } from './donations/PricingSection';
import { AccountsSection } from './sections/Accounts';
import { BansSection } from './sections/Bans';
import { CliParitySection } from './sections/CliParity';
import { DashboardSection } from './sections/Dashboard';
import { DataRequestsSection } from './sections/DataRequests';
import { EntranceSection } from './sections/Entrance';
import { FeaturesSection } from './sections/Features';
import { InviteGraphSection } from './sections/InviteGraph';
import { InvitesSection } from './sections/Invites';
import { LimitsSection } from './sections/Limits';
import { ServersSection } from './sections/Servers';
import { StaffSection } from './sections/Staff';
import { ReviewHubSection } from './sections/ReviewHub';
import { PrivateContentSection } from './sections/PrivateContent';
import { JobsSection, SystemSection } from './sections/System';
import { AdminViewerContext, Loading, errorText, makeViewer, type AdminViewer } from './ui';

interface SectionDef {
  id: string;
  label: string;
  group: string;
  visible: (v: AdminViewer) => boolean;
  component: ComponentType;
}

export const ADMIN_SECTIONS: SectionDef[] = [
  {
    id: 'dashboard',
    label: 'admin.adminApp.section.dashboard',
    group: 'admin.adminApp.group.overview',
    visible: (v) => v.can('STAFF_BASIC'),
    component: DashboardSection,
  },
  {
    id: 'accounts',
    label: 'admin.adminApp.section.accounts',
    group: 'admin.adminApp.group.people',
    visible: (v) => v.can('MANAGE_ACCOUNTS'),
    component: AccountsSection,
  },
  {
    id: 'bans',
    label: 'admin.adminApp.section.bans',
    group: 'admin.adminApp.group.people',
    visible: (v) => v.can('PLATFORM_BAN'),
    component: BansSection,
  },
  {
    id: 'invites',
    label: 'admin.adminApp.section.invites',
    group: 'admin.adminApp.group.people',
    visible: (v) => v.can('MANAGE_INVITES'),
    component: InvitesSection,
  },
  {
    id: 'invite-graph',
    label: 'admin.adminApp.section.inviteGraph',
    group: 'admin.adminApp.group.people',
    visible: (v) => v.can('MANAGE_INVITES'),
    component: InviteGraphSection,
  },
  {
    id: 'servers',
    label: 'admin.adminApp.section.servers',
    group: 'admin.adminApp.group.people',
    visible: (v) => v.can('MANAGE_GUILDS_GLOBAL'),
    component: ServersSection,
  },
  {
    id: 'staff',
    label: 'admin.adminApp.section.staff',
    group: 'admin.adminApp.group.people',
    visible: (v) => v.can('APPOINT_STAFF'),
    component: StaffSection,
  },
  {
    id: 'review',
    label: 'admin.adminApp.section.review',
    group: 'admin.adminApp.group.content',
    visible: (v) =>
      hasAny(
        v.perms,
        'REVIEW_MEDIA',
        'REVIEW_READING',
        'DELETE_CONTENT',
        'REVIEW_APPS',
        'REVIEW_PACKAGES',
        'MANAGE_DIRECTORY',
      ),
    component: ReviewHubSection,
  },
  {
    id: 'jobs',
    label: 'admin.adminApp.section.jobs',
    group: 'admin.adminApp.group.content',
    visible: (v) => v.can('MANAGE_TRANSCODE'),
    component: JobsSection,
  },
  {
    id: 'ingest-sources',
    label: 'admin.adminApp.section.ingestSources',
    group: 'admin.adminApp.group.content',
    visible: (v) => v.can('PUBLISH_READING'),
    component: IngestSourcesSection,
  },
  {
    id: 'library-catalog',
    label: 'admin.adminApp.section.libraryCatalog',
    group: 'admin.adminApp.group.content',
    visible: (v) => v.can('PUBLISH_READING'),
    component: LibraryCatalogSection,
  },
  {
    id: 'cinema-catalog',
    label: 'admin.adminApp.section.cinemaCatalog',
    group: 'admin.adminApp.group.cinema',
    visible: (v) => hasAny(v.perms, 'PUBLISH_MEDIA', 'DELETE_CONTENT'),
    component: CatalogSection,
  },
  {
    id: 'cinema-collections',
    label: 'admin.adminApp.section.cinemaCollections',
    group: 'admin.adminApp.group.cinema',
    visible: (v) => v.can('MANAGE_COLLECTIONS'),
    component: CollectionsSection,
  },
  {
    id: 'cinema-transcoding',
    label: 'admin.adminApp.section.cinemaTranscoding',
    group: 'admin.adminApp.group.cinema',
    visible: (v) => v.can('MANAGE_TRANSCODE'),
    component: TranscodingSection,
  },
  {
    id: 'cinema-live-tv',
    label: 'admin.adminApp.section.cinemaLiveTv',
    group: 'admin.adminApp.group.cinema',
    visible: (v) => v.can('MANAGE_TV'),
    component: LiveTvSection,
  },
  {
    id: 'donations',
    label: 'admin.adminApp.section.donations',
    group: 'admin.adminApp.group.money',
    visible: (v) => v.can('MANAGE_DONATIONS'),
    component: DonationsOverviewSection,
  },
  {
    id: 'donation-invoices',
    label: 'admin.adminApp.section.donationInvoices',
    group: 'admin.adminApp.group.money',
    visible: (v) => v.can('MANAGE_DONATIONS'),
    component: InvoicesSection,
  },
  {
    id: 'donation-entitlements',
    label: 'admin.adminApp.section.donationEntitlements',
    group: 'admin.adminApp.group.money',
    visible: (v) => v.can('MANAGE_DONATIONS'),
    component: EntitlementsSection,
  },
  {
    id: 'donation-pricing',
    label: 'admin.adminApp.section.donationPricing',
    group: 'admin.adminApp.group.money',
    visible: (v) => v.can('MANAGE_DONATIONS'),
    component: PricingSection,
  },
  {
    id: 'features',
    label: 'admin.adminApp.section.features',
    group: 'admin.adminApp.group.platform',
    visible: (v) => v.can('MANAGE_FEATURES'),
    component: FeaturesSection,
  },
  {
    id: 'limits',
    label: 'admin.adminApp.section.limits',
    group: 'admin.adminApp.group.platform',
    visible: (v) => v.can('MANAGE_FEATURES'),
    component: LimitsSection,
  },
  {
    id: 'entrance',
    label: 'admin.adminApp.section.entrance',
    group: 'admin.adminApp.group.platform',
    visible: (v) => v.can('MANAGE_ENTRANCE'),
    component: EntranceSection,
  },
  {
    id: 'announcements',
    label: 'admin.adminApp.section.announcements',
    group: 'admin.adminApp.group.platform',
    visible: (v) => v.can('MANAGE_FEATURES'),
    component: AnnouncementsSection,
  },
  {
    id: 'system',
    label: 'admin.adminApp.section.system',
    group: 'admin.adminApp.group.platform',
    visible: (v) => v.can('OPERATOR'),
    component: SystemSection,
  },
  {
    id: 'audit-log',
    label: 'admin.adminApp.section.auditLog',
    group: 'admin.adminApp.group.audit',
    visible: (v) => v.can('VIEW_ACCESS_LOG'),
    component: AuditLogSection,
  },
  {
    id: 'private-content',
    label: 'admin.adminApp.section.privateContent',
    group: 'admin.adminApp.group.audit',
    visible: (v) => v.can('READ_PRIVATE_CONTENT'),
    component: PrivateContentSection,
  },
  {
    id: 'access-log',
    label: 'admin.adminApp.section.accessLog',
    group: 'admin.adminApp.group.audit',
    visible: (v) => v.can('VIEW_ACCESS_LOG'),
    component: AccessLogSection,
  },
  {
    id: 'cli',
    label: 'admin.adminApp.section.cli',
    group: 'admin.adminApp.group.audit',
    visible: (v) =>
      hasAny(v.perms, 'MANAGE_INVITES', 'APPOINT_STAFF', 'OPERATOR', 'VIEW_ACCESS_LOG'),
    component: CliParitySection,
  },
  {
    // M-25 step c: its own page until the integrator places it under Admin > Moderation.
    id: 'data-requests',
    label: 'admin.adminApp.section.dataRequests',
    group: 'admin.dataRequests.group',
    visible: (v) => v.can('MANAGE_TAKEDOWNS'),
    component: DataRequestsSection,
  },
];

function useMe() {
  const [state, setState] = useState<{ data?: api.Me; error?: string; status?: number }>({});
  useEffect(() => {
    let live = true;
    api
      .getMe()
      .then((data) => live && setState({ data }))
      .catch(
        (e: unknown) =>
          live &&
          setState({
            error: errorText(e, translate('admin.adminApp.staffAccessError')),
            ...(e instanceof ApiError ? { status: e.status } : {}),
          }),
      );
    return () => {
      live = false;
    };
  }, []);
  return state;
}

export function AdminShell({ section }: { section: string | undefined }) {
  const t = useT();
  const navigate = useNavigate();
  const me = useMe();
  const viewer = useMemo(
    () =>
      me.data
        ? makeViewer(me.data.user_id, me.data.is_owner, parsePerms(me.data.platform_permissions))
        : null,
    [me.data],
  );
  const visible = useMemo(
    () => (viewer ? ADMIN_SECTIONS.filter((s) => s.visible(viewer)) : []),
    [viewer],
  );
  const close = () => void navigate({ to: '/channels/$guildId', params: { guildId: '@me' } });
  // A revoked appointment ends the visit at once (ADM-STF-005).
  useStaffRevoked(close);

  const navSections: SettingsSection[] = visible.map((s) => ({
    id: s.id,
    label: t(s.label),
    group: t(s.group),
  }));
  const active = visible.find((s) => s.id === section);

  let body;
  if (me.error && !me.data) {
    // /admin/me answers 401 when signed out and 403 for accounts that are not staff.
    const signedOut = me.status === 401;
    body = signedOut ? (
      <EmptyState
        title={t('admin.adminApp.signIn.title')}
        description={t('admin.adminApp.signIn.description')}
        action={
          <Button onClick={() => void navigate({ to: '/login', search: { next: '/admin' } })}>
            {t('admin.adminApp.signIn.button')}
          </Button>
        }
      />
    ) : (
      <EmptyState
        title={t('admin.adminApp.staffOnly.title')}
        description={me.error}
        action={<Button onClick={close}>{t('admin.adminApp.backToLounge')}</Button>}
      />
    );
  } else if (!viewer) {
    body = <Loading label={t('admin.adminApp.loading')} />;
  } else if (visible.length === 0) {
    body = (
      <EmptyState
        title={t('admin.adminApp.nothing.title')}
        description={t('admin.adminApp.nothing.description')}
        action={<Button onClick={close}>{t('admin.adminApp.backToLounge')}</Button>}
      />
    );
  } else if (!active) {
    return <Navigate to="/admin/$section" params={{ section: visible[0]!.id }} replace />;
  } else {
    const Section = active.component;
    body = (
      <AdminViewerContext.Provider value={viewer}>
        <Section key={active.id} />
      </AdminViewerContext.Provider>
    );
  }

  return (
    <>
      <SettingsLayout
        title={t('admin.adminApp.title')}
        sections={navSections}
        activeId={active?.id ?? ''}
        onSelect={(id) => void navigate({ to: '/admin/$section', params: { section: id } })}
        onClose={close}
        wide
      >
        {body}
      </SettingsLayout>
      <ReauthHost />
    </>
  );
}

/** Route component: provides the staff theme and toast context the full-screen page needs. */
export function AdminPage() {
  const { section } = useParams({ strict: false }) as { section?: string };
  return (
    <ThemeProvider section="staff" className="ss-root">
      <ToastProvider>
        <AdminShell section={section} />
      </ToastProvider>
    </ThemeProvider>
  );
}
