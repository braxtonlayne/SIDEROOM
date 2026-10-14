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
import * as api from './api';
import { hasAny, parsePerms } from './permissions';
import { AccessLogSection } from './sections/AccessLog';
import { AccountsSection } from './sections/Accounts';
import { BansSection } from './sections/Bans';
import { CliParitySection } from './sections/CliParity';
import { DashboardSection } from './sections/Dashboard';
import { FeaturesSection } from './sections/Features';
import { InvitesSection } from './sections/Invites';
import { LimitsSection } from './sections/Limits';
import { StaffSection } from './sections/Staff';
import { ReviewSection } from './sections/Review';
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
    label: 'Dashboard',
    group: 'OVERVIEW',
    visible: (v) => v.can('STAFF_BASIC'),
    component: DashboardSection,
  },
  {
    id: 'accounts',
    label: 'Accounts',
    group: 'PEOPLE',
    visible: (v) => v.can('MANAGE_ACCOUNTS'),
    component: AccountsSection,
  },
  {
    id: 'bans',
    label: 'Platform bans',
    group: 'PEOPLE',
    visible: (v) => v.can('PLATFORM_BAN'),
    component: BansSection,
  },
  {
    id: 'invites',
    label: 'Invites',
    group: 'PEOPLE',
    visible: (v) => v.can('MANAGE_INVITES'),
    component: InvitesSection,
  },
  {
    id: 'staff',
    label: 'Staff & roles',
    group: 'PEOPLE',
    visible: (v) => v.can('APPOINT_STAFF'),
    component: StaffSection,
  },
  {
    id: 'review',
    label: 'Review queues',
    group: 'CONTENT',
    visible: (v) =>
      hasAny(
        v.perms,
        'REVIEW_MEDIA',
        'REVIEW_READING',
        'REVIEW_APPS',
        'REVIEW_PACKAGES',
        'MANAGE_DIRECTORY',
      ),
    component: ReviewSection,
  },
  {
    id: 'jobs',
    label: 'Jobs',
    group: 'CONTENT',
    visible: (v) => v.can('MANAGE_TRANSCODE'),
    component: JobsSection,
  },
  {
    id: 'features',
    label: 'Features',
    group: 'PLATFORM',
    visible: (v) => v.can('MANAGE_FEATURES'),
    component: FeaturesSection,
  },
  {
    id: 'limits',
    label: 'Limits & settings',
    group: 'PLATFORM',
    visible: (v) => v.can('MANAGE_FEATURES'),
    component: LimitsSection,
  },
  {
    id: 'system',
    label: 'System',
    group: 'PLATFORM',
    visible: (v) => v.can('OPERATOR'),
    component: SystemSection,
  },
  {
    id: 'access-log',
    label: 'Access log',
    group: 'AUDIT',
    visible: (v) => v.can('VIEW_ACCESS_LOG'),
    component: AccessLogSection,
  },
  {
    id: 'cli',
    label: 'Command line',
    group: 'AUDIT',
    visible: (v) =>
      hasAny(v.perms, 'MANAGE_INVITES', 'APPOINT_STAFF', 'OPERATOR', 'VIEW_ACCESS_LOG'),
    component: CliParitySection,
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
            error: errorText(e, 'Could not check your staff access.'),
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

  const navSections: SettingsSection[] = visible.map((s) => ({
    id: s.id,
    label: s.label,
    group: s.group,
  }));
  const active = visible.find((s) => s.id === section);

  let body;
  if (me.error && !me.data) {
    // /admin/me answers 401 when signed out and 403 for accounts that are not staff.
    const signedOut = me.status === 401;
    body = signedOut ? (
      <EmptyState
        title="Sign in to continue"
        description="The admin pages need a staff session."
        action={
          <Button onClick={() => void navigate({ to: '/login', search: { next: '/admin' } })}>
            Log in
          </Button>
        }
      />
    ) : (
      <EmptyState
        title="Staff only"
        description={me.error}
        action={<Button onClick={close}>Back to the Lounge</Button>}
      />
    );
  } else if (!viewer) {
    body = <Loading label="Loading admin" />;
  } else if (visible.length === 0) {
    body = (
      <EmptyState
        title="Nothing to administer"
        description="Your staff roles do not include any admin page. Ask the Owner or a senior staff member to adjust them."
        action={<Button onClick={close}>Back to the Lounge</Button>}
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
    <SettingsLayout
      title="Platform admin"
      sections={navSections}
      activeId={active?.id ?? ''}
      onSelect={(id) => void navigate({ to: '/admin/$section', params: { section: id } })}
      onClose={close}
      wide
    >
      {body}
    </SettingsLayout>
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
