/**
 * Full-screen Server Settings (LNG-SSET-001/002). Route: /settings/server/:guildId/:section.
 * The shell owns the section list (filtered by the viewer's permissions), the unsaved-changes bar and the
 * ESC/close behaviour; sections publish their dirty state with `useSettingsForm`.
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
import { useCallback, useEffect, useMemo, useState, type ComponentType } from 'react';
import { LiveDataConnection, useEnsureGuild } from '../../lounge/live';
import { useGuild, useStore } from '../../store';
import { FormContext, useViewer, type FormHandle, type Viewer } from './hooks';
import { Loading } from './ui';
import { AuditLogSection } from './sections/AuditLog';
import { BansSection } from './sections/Bans';
import { ChannelsSection } from './sections/Channels';
import { CommunitySection } from './sections/Community';
import { DeleteServerSection } from './sections/DeleteServer';
import { ExpressionsSection } from './sections/Expressions';
import { IntegrationsSection } from './sections/Integrations';
import { InvitesSection } from './sections/Invites';
import { MembersSection } from './sections/Members';
import { ModerationSection } from './sections/Moderation';
import { OverviewSection } from './sections/Overview';
import { RolesSection } from './sections/Roles';
import { ServerSupportSection } from './sections/ServerSupport';

export interface SectionProps {
  guildId: string;
  viewer: Viewer;
}

interface SectionDef {
  id: string;
  label: string;
  group: string;
  visible: (v: Viewer) => boolean;
  component: ComponentType<SectionProps>;
  danger?: boolean;
}

const any = (v: Viewer, ...names: string[]) => names.some((n) => v.can(n));

export const SECTIONS: SectionDef[] = [
  {
    id: 'overview',
    label: 'Overview',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: OverviewSection,
  },
  {
    id: 'roles',
    label: 'Roles',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_ROLES'),
    component: RolesSection,
  },
  {
    id: 'support',
    label: 'Server Support',
    group: 'SERVER',
    visible: (v) => v.isOwner || v.can('MANAGE_GUILD'),
    component: ServerSupportSection,
  },
  {
    id: 'channels',
    label: 'Channels',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_CHANNELS'),
    component: ChannelsSection,
  },
  {
    id: 'emoji',
    label: 'Emoji & Stickers',
    group: 'EXPRESSION',
    visible: (v) => v.can('MANAGE_GUILD_EXPRESSIONS'),
    component: ExpressionsSection,
  },
  {
    id: 'members',
    label: 'Members',
    group: 'PEOPLE',
    visible: (v) => any(v, 'KICK_MEMBERS', 'BAN_MEMBERS', 'MODERATE_MEMBERS', 'MANAGE_ROLES'),
    component: MembersSection,
  },
  {
    id: 'invites',
    label: 'Invites',
    group: 'PEOPLE',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: InvitesSection,
  },
  {
    id: 'bans',
    label: 'Bans',
    group: 'PEOPLE',
    visible: (v) => v.can('BAN_MEMBERS'),
    component: BansSection,
  },
  {
    id: 'integrations',
    label: 'Integrations',
    group: 'APPS',
    visible: (v) => any(v, 'MANAGE_WEBHOOKS', 'MANAGE_GUILD'),
    component: IntegrationsSection,
  },
  {
    id: 'moderation',
    label: 'AutoMod',
    group: 'MODERATION',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: ModerationSection,
  },
  {
    id: 'audit-log',
    label: 'Audit Log',
    group: 'MODERATION',
    visible: (v) => v.can('VIEW_AUDIT_LOG'),
    component: AuditLogSection,
  },
  {
    id: 'community',
    label: 'Community',
    group: 'COMMUNITY',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: CommunitySection,
  },
  {
    id: 'delete',
    label: 'Delete Server',
    group: 'DANGER',
    visible: (v) => v.isOwner,
    component: DeleteServerSection,
    danger: true,
  },
];

/** Can the viewer open server settings at all (used by the server dropdown)? */
export const canOpenServerSettings = (v: Viewer) =>
  SECTIONS.some((s) => s.id !== 'delete' && s.visible(v));

export function ServerSettings({
  guildId,
  section,
}: {
  guildId: string;
  section: string | undefined;
}) {
  const navigate = useNavigate();
  const { guild } = useGuild(guildId);
  const viewer = useViewer(guildId);
  const [form, setForm] = useState<FormHandle | null>(null);
  useEnsureGuild(guildId, true);
  const loadMembers = useStore((s) => s.loadMembers);
  useEffect(() => {
    loadMembers(guildId).catch(() => undefined);
  }, [guildId, loadMembers]);

  const visible = useMemo(
    () => (viewer.ready ? SECTIONS.filter((s) => s.visible(viewer)) : []),
    [viewer],
  );
  const close = useCallback(() => {
    void navigate({ to: '/channels/$guildId', params: { guildId } });
  }, [navigate, guildId]);
  const go = useCallback(
    (id: string) => {
      setForm(null);
      void navigate({ to: '/settings/server/$guildId/$section', params: { guildId, section: id } });
    },
    [navigate, guildId],
  );

  const navSections: SettingsSection[] = visible.map((s) => ({
    id: s.id,
    label: s.label,
    group:
      s.id === 'delete'
        ? 'DANGER ZONE'
        : s.group === 'SERVER'
          ? (guild?.name ?? 'SERVER').toUpperCase()
          : s.group,
    ...(s.danger ? { danger: true } : {}),
  }));

  const active = visible.find((s) => s.id === section);
  let body;
  if (!viewer.ready || !guild) {
    body = <Loading label="Loading server settings" />;
  } else if (visible.length === 0) {
    body = (
      <EmptyState
        title="You do not have access to server settings"
        description="Ask a server admin for a role with Manage Server, Manage Roles, Manage Channels, Ban Members or View Audit Log."
        action={<Button onClick={close}>Back to the server</Button>}
      />
    );
  } else if (!active) {
    return (
      <Navigate
        to="/settings/server/$guildId/$section"
        params={{ guildId, section: visible[0]!.id }}
        replace
      />
    );
  } else {
    const Section = active.component;
    body = <Section key={`${guildId}:${active.id}`} guildId={guildId} viewer={viewer} />;
  }

  return (
    <FormContext.Provider value={setForm}>
      <SettingsLayout
        title={`${guild?.name ?? 'Server'} settings`}
        sections={navSections}
        activeId={active?.id ?? ''}
        onSelect={go}
        onClose={close}
        dirty={!!form?.dirty}
        saving={!!form?.saving}
        onSave={() => form?.save()}
        onReset={() => form?.reset()}
        blockNavWhenDirty
        wide
      >
        {body}
      </SettingsLayout>
    </FormContext.Provider>
  );
}

/** Route component: reads the params and provides the theme/toast context the full-screen page needs. */
export function ServerSettingsPage() {
  const { guildId, section } = useParams({ strict: false }) as {
    guildId?: string;
    section?: string;
  };
  return (
    <LiveDataConnection>
      <ThemeProvider section="lounge" className="ss-root">
        <ToastProvider>
          {guildId ? <ServerSettings guildId={guildId} section={section} /> : null}
        </ToastProvider>
      </ThemeProvider>
    </LiveDataConnection>
  );
}
