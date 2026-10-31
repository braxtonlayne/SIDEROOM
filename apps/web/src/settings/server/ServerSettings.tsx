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
import { useT, type MessageKey } from '../../i18n';
import { LiveDataConnection, useEnsureGuild } from '../../lounge/live';
import { useGuild, useStore } from '../../store';
import { FormContext, useViewer, type FormHandle, type Viewer } from './hooks';
import { Loading } from './ui';
import { AuditLogSection } from './sections/AuditLog';
import { BansSection } from './sections/Bans';
import { BridgeSection } from './bridge/BridgeSection';
import { useBridgeNav } from './bridge/moduleStore';
import { ChannelsSection } from './sections/Channels';
import { CommunitySection } from './sections/Community';
import { DeleteServerSection } from './sections/DeleteServer';
import { ExpressionsSection } from './sections/Expressions';
import { FeaturesSection } from './sections/Features';
import { ExtensionsSection } from './sections/Extensions';
import { IntegrationsSection } from './sections/Integrations';
import { InvitesSection } from './sections/Invites';
import { MembersSection } from './sections/Members';
import { ModerationSection } from './sections/Moderation';
import { OverviewSection } from './sections/Overview';
import { RolesSection } from './sections/Roles';
import { ServerSupportSection } from './sections/ServerSupport';
import { TagSection } from './sections/Tag';
import { ServerStorageSection } from './storage/StorageSection';
import { WatchLibrarySection } from './sections/WatchLibrary';

export interface SectionProps {
  guildId: string;
  viewer: Viewer;
}

interface SectionCtx {
  bridgeOn: boolean;
}

interface SectionDef {
  id: string;
  /** Message key of the section label. */
  label: MessageKey;
  group: string;
  /** `ctx` carries what the viewer alone cannot say (the Discord bridge entry exists only while the module is on). */
  visible: (v: Viewer, ctx?: SectionCtx) => boolean;
  component: ComponentType<SectionProps>;
  danger?: boolean;
}

const GROUP_KEYS: Record<string, MessageKey> = {
  SERVER: 'settings.serverSettings.groupServer',
  EXPRESSION: 'settings.serverSettings.groupExpression',
  PEOPLE: 'settings.serverSettings.groupPeople',
  APPS: 'settings.serverSettings.groupApps',
  MODERATION: 'settings.serverSettings.groupModeration',
  COMMUNITY: 'settings.serverSettings.groupCommunity',
  DANGER: 'settings.serverSettings.groupDanger',
};

const any = (v: Viewer, ...names: string[]) => names.some((n) => v.can(n));

export const SECTIONS: SectionDef[] = [
  {
    id: 'overview',
    label: 'settings.server.overview',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: OverviewSection,
  },
  {
    id: 'roles',
    label: 'settings.server.roles',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_ROLES'),
    component: RolesSection,
  },
  {
    id: 'support',
    label: 'settings.serverSettings.support',
    group: 'SERVER',
    visible: (v) => v.isOwner || v.can('MANAGE_GUILD'),
    component: ServerSupportSection,
  },
  {
    id: 'tag',
    label: 'settings.serverTag.nav',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: TagSection,
  },
  {
    id: 'channels',
    label: 'settings.server.channels',
    group: 'SERVER',
    visible: (v) => v.can('MANAGE_CHANNELS'),
    component: ChannelsSection,
  },
  {
    id: 'emoji',
    label: 'settings.server.emoji',
    group: 'EXPRESSION',
    visible: (v) => v.can('MANAGE_GUILD_EXPRESSIONS'),
    component: ExpressionsSection,
  },
  {
    id: 'members',
    label: 'settings.server.members',
    group: 'PEOPLE',
    visible: (v) => any(v, 'KICK_MEMBERS', 'BAN_MEMBERS', 'MODERATE_MEMBERS', 'MANAGE_ROLES'),
    component: MembersSection,
  },
  {
    id: 'invites',
    label: 'settings.server.invites',
    group: 'PEOPLE',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: InvitesSection,
  },
  {
    id: 'bans',
    label: 'settings.server.bans',
    group: 'PEOPLE',
    visible: (v) => v.can('BAN_MEMBERS'),
    component: BansSection,
  },
  {
    id: 'features',
    label: 'settings.server.features',
    group: 'APPS',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: FeaturesSection,
  },
  {
    id: 'extensions',
    label: 'settings.server.extensions',
    group: 'APPS',
    // Packs need MANAGE_GUILD_EXPRESSIONS alone; themes, recipes and templates keep MANAGE_GUILD (D-282).
    visible: (v) => any(v, 'MANAGE_GUILD', 'MANAGE_GUILD_EXPRESSIONS'),
    component: ExtensionsSection,
  },
  {
    id: 'watch-library',
    label: 'settings.server.watchLibrary',
    group: 'APPS',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: WatchLibrarySection,
  },
  {
    id: 'storage',
    label: 'settings.serverStorage.nav',
    group: 'APPS',
    // MANAGE_GUILD implies the `manage` storage grant (roadmap-p27 section 3); the server decides.
    visible: (v) => v.can('MANAGE_GUILD'),
    component: ServerStorageSection,
  },
  {
    id: 'integrations',
    label: 'settings.server.integrations',
    group: 'APPS',
    visible: (v) => any(v, 'MANAGE_WEBHOOKS', 'MANAGE_GUILD'),
    component: IntegrationsSection,
  },
  {
    id: 'bridge',
    label: 'settings.bridge.nav',
    group: 'APPS',
    // BRG-UI-001 (D-381): out of the way until the owner switches the module on; then the owner and MANAGE_GUILD.
    visible: (v, ctx) => !!ctx?.bridgeOn && (v.isOwner || v.can('MANAGE_GUILD')),
    component: BridgeSection,
  },
  {
    id: 'moderation',
    label: 'settings.server.automod',
    group: 'MODERATION',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: ModerationSection,
  },
  {
    id: 'audit-log',
    label: 'settings.server.audit',
    group: 'MODERATION',
    visible: (v) => v.can('VIEW_AUDIT_LOG'),
    component: AuditLogSection,
  },
  {
    id: 'community',
    label: 'settings.server.community',
    group: 'COMMUNITY',
    visible: (v) => v.can('MANAGE_GUILD'),
    component: CommunitySection,
  },
  {
    id: 'delete',
    label: 'settings.server.delete',
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
  const t = useT();
  const navigate = useNavigate();
  const { guild } = useGuild(guildId);
  const viewer = useViewer(guildId);
  const [form, setForm] = useState<FormHandle | null>(null);
  useEnsureGuild(guildId, true);
  const loadMembers = useStore((s) => s.loadMembers);
  useEffect(() => {
    loadMembers(guildId).catch(() => undefined);
  }, [guildId, loadMembers]);

  const { visible: bridgeOn, pending: bridgePending } = useBridgeNav(guildId, viewer);
  const visible = useMemo(
    () => (viewer.ready ? SECTIONS.filter((s) => s.visible(viewer, { bridgeOn })) : []),
    [viewer, bridgeOn],
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
    label: t(s.label),
    group:
      s.id === 'delete'
        ? t('settings.serverSettings.dangerZone')
        : s.group === 'SERVER'
          ? (guild?.name ?? t('settings.serverSettings.groupServer')).toUpperCase()
          : t(GROUP_KEYS[s.group] ?? 'settings.serverSettings.groupServer'),
    ...(s.danger ? { danger: true } : {}),
  }));

  const active = visible.find((s) => s.id === section);
  let body;
  if (!viewer.ready || !guild) {
    body = <Loading label={t('settings.serverSettings.loading')} />;
  } else if (visible.length === 0) {
    body = (
      <EmptyState
        title={t('settings.serverSettings.noAccessTitle')}
        description={t('settings.serverSettings.noAccessDescription')}
        action={<Button onClick={close}>{t('settings.serverSettings.backToServer')}</Button>}
      />
    );
  } else if (!active && bridgePending && section === 'bridge') {
    // A deep link to the bridge screen waits for the module state instead of being sent to the first section.
    body = <Loading label={t('settings.serverSettings.loading')} />;
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
        title={t('settings.serverSettings.title', {
          name: guild?.name ?? t('settings.serverSettings.defaultName'),
        })}
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
