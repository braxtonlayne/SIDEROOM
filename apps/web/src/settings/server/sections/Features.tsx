// Server Settings > Features (FTR-*): the built-in feature modules of this server (starboard, welcome messages,
// reaction roles, sticky messages, ...). Every module is a switch plus a settings form the server describes
// itself (crates/sr-features), so a new module needs no screen here. Temporary roles also get a small panel
// to grant and end roles, because they are actions as well as settings.
import { Button, Notice, Select, Switch } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import {
  SchemaForm,
  overridesOf,
  schemaFields,
  type PickerChoices,
  type SettingsValues,
} from '../../../admin/SchemaForm';
import { formatDateTime, useT } from '../../../i18n';
import { useChannels, useGuild, useMembers } from '../../../store';
import * as api from '../featuresApi';
import { useResource } from '../hooks';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';
import type { SectionProps } from '../ServerSettings';
import { SlowmodeForm } from '../../../lounge/features/SlowmodeForm';
import { MessagePicker } from './MessagePicker';
import { RssPanel } from './RssFeeds';

const TEMP_ROLES_ID = 'moderation.temp_roles';
const RSS_ID = 'community.rss';
const SLOWMODE_ID = 'lounge.slowmode_plus';
const REACTION_ROLES_ID = 'lounge.reaction_roles';
/** Channel type code of a category; it holds channels and is not somewhere a module can post. */
const CATEGORY = 4;

/** Grant lengths offered in the temporary-role form, in seconds. */
const DURATIONS = [
  { seconds: 3600, key: 'settings.features.temp.hour' },
  { seconds: 86400, key: 'settings.features.temp.day' },
  { seconds: 7 * 86400, key: 'settings.features.temp.week' },
  { seconds: 30 * 86400, key: 'settings.features.temp.month' },
] as const;

export function FeaturesSection({ guildId }: SectionProps) {
  const t = useT();
  const res = useResource(`features-${guildId}`, () => api.listFeatures(guildId));
  const channels = useChannels(guildId);
  const { roles } = useGuild(guildId);
  const pickers = useMemo<PickerChoices>(
    () => ({
      channels: channels
        .filter((c) => c.type !== CATEGORY)
        .map((c) => ({ id: c.id, name: c.name ?? c.id })),
      roles: roles.filter((r) => r.id !== guildId).map((r) => ({ id: r.id, name: r.name })),
    }),
    [channels, roles, guildId],
  );

  return (
    <>
      <SectionHeader
        title={t('settings.features.title')}
        description={t('settings.features.description')}
      />
      <Async res={res} label={t('settings.features.loading')}>
        {(features) => {
          const bySection = new Map<string, api.GuildFeature[]>();
          for (const f of features)
            bySection.set(f.section, [...(bySection.get(f.section) ?? []), f]);
          if (features.length === 0)
            return <p className="ss-meta">{t('settings.features.none')}</p>;
          return (
            <>
              {[...bySection.entries()].map(([section, list]) => (
                <section key={section} aria-label={section}>
                  <h2 className="ss-sub">{section}</h2>
                  {list.map((f) => (
                    <FeatureCard
                      key={f.id}
                      guildId={guildId}
                      feature={f}
                      pickers={pickers}
                      onChanged={(next) =>
                        res.set((cur) => (cur ?? []).map((x) => (x.id === next.id ? next : x)))
                      }
                    />
                  ))}
                </section>
              ))}
            </>
          );
        }}
      </Async>
    </>
  );
}

function FeatureCard({
  guildId,
  feature,
  pickers,
  onChanged,
}: {
  guildId: string;
  feature: api.GuildFeature;
  pickers: PickerChoices;
  onChanged: (next: api.GuildFeature) => void;
}) {
  const t = useT();
  const fields = useMemo(() => schemaFields(feature.schema), [feature.schema]);
  const [values, setValues] = useState<SettingsValues>(feature.settings);
  const [open, setOpen] = useState(false);
  const act = useAction();
  const dirty =
    JSON.stringify(overridesOf(fields, values)) !==
    JSON.stringify(overridesOf(fields, feature.settings));

  const toggle = (enabled: boolean) =>
    void act.run(async () => onChanged(await api.patchFeature(guildId, feature.id, { enabled })));
  const save = () =>
    void act.run(async () => {
      const next = await api.patchFeature(guildId, feature.id, {
        settings: overridesOf(fields, values),
      });
      setValues(next.settings);
      onChanged(next);
    }, t('settings.features.saved'));
  const id = `feature-${feature.id.replace(/[^a-z0-9]/gi, '-')}`;

  return (
    <article className="ss-card" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="ss-card-title">
        {feature.name}
      </h3>
      <p className="ss-desc">{feature.description}</p>
      {feature.platform_disabled && (
        <Notice variant="warning">{t('settings.features.platformOff')}</Notice>
      )}
      <Switch
        label={t('settings.features.enable', { name: feature.name })}
        checked={feature.enabled}
        disabled={act.busy}
        onChange={(e) => toggle(e.target.checked)}
      />
      {fields.length > 0 && (
        <>
          <Button
            size="sm"
            variant="secondary"
            aria-expanded={open}
            aria-controls={`${id}-form`}
            onClick={() => setOpen((o) => !o)}
          >
            {open
              ? t('settings.features.hideSettings', { name: feature.name })
              : t('settings.features.showSettings', { name: feature.name })}
          </Button>
          {open && (
            <div id={`${id}-form`}>
              <SchemaForm
                fields={fields}
                values={values}
                pickers={pickers}
                onChange={setValues}
                {...(feature.id === REACTION_ROLES_ID
                  ? {
                      customText: (f, value, set) =>
                        f.key === 'message' ? (
                          <MessagePicker
                            guildId={guildId}
                            label={f.label}
                            value={value}
                            onChange={set}
                          />
                        ) : undefined,
                    }
                  : {})}
              />
              <div className="ss-actions">
                <Button size="sm" disabled={!dirty || act.busy} onClick={save}>
                  {t('settings.features.save', { name: feature.name })}
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={!dirty || act.busy}
                  onClick={() => setValues(feature.settings)}
                >
                  {t('settings.features.reset', { name: feature.name })}
                </Button>
              </div>
            </div>
          )}
        </>
      )}
      <ActionStatus error={act.error} done={act.done} />
      {feature.id === TEMP_ROLES_ID && feature.enabled && <TempRolesPanel guildId={guildId} />}
      {feature.id === RSS_ID && feature.enabled && <RssPanel guildId={guildId} />}
      {feature.id === SLOWMODE_ID && feature.enabled && (
        <section aria-label={t('lounge.slowmode.panelTitle')}>
          <h4 className="ss-sub">{t('lounge.slowmode.panelTitle')}</h4>
          <p className="ss-meta">{t('lounge.slowmode.panelHint')}</p>
          <SlowmodeForm guildId={guildId} />
        </section>
      )}
    </article>
  );
}

function TempRolesPanel({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`temp-roles-${guildId}`, () => api.listTempRoles(guildId));
  const { roles } = useGuild(guildId);
  const { members } = useMembers(guildId);
  const [user, setUser] = useState('');
  const [role, setRole] = useState('');
  const [secs, setSecs] = useState<number>(DURATIONS[1].seconds);
  const act = useAction();

  const userName = (id: string) => {
    const u = members.find((m) => m.member.user_id === id)?.user;
    return u?.global_name || u?.username || id;
  };
  const roleName = (id: string) => roles.find((r) => r.id === id)?.name ?? id;

  const grant = () =>
    void act.run(async () => {
      await api.grantTempRole(guildId, { user_id: user, role_id: role, duration_seconds: secs });
      await res.reload();
    }, t('settings.features.temp.granted'));
  const revoke = (g: api.TempRole) =>
    void act.run(async () => {
      await api.revokeTempRole(guildId, g.user_id, g.role_id);
      await res.reload();
    });

  return (
    <section aria-label={t('settings.features.temp.title')}>
      <h4 className="ss-sub">{t('settings.features.temp.title')}</h4>
      <Async res={res} label={t('settings.features.loading')}>
        {(grants) =>
          grants.length === 0 ? (
            <p className="ss-meta">{t('settings.features.temp.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('settings.features.temp.title')}>
              {grants.map((g) => (
                <li key={`${g.user_id}:${g.role_id}`} className="ss-row">
                  <span>
                    {t('settings.features.temp.line', {
                      user: userName(g.user_id),
                      role: roleName(g.role_id),
                      until: formatDateTime(g.expires_at),
                    })}
                  </span>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={act.busy}
                    aria-label={t('settings.features.temp.revoke', {
                      user: userName(g.user_id),
                      role: roleName(g.role_id),
                    })}
                    onClick={() => revoke(g)}
                  >
                    {t('settings.features.temp.end')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <div className="adm-form">
        <Select
          aria-label={t('settings.features.temp.member')}
          placeholder={t('settings.features.temp.member')}
          searchable
          options={members.map((m) => ({
            value: m.member.user_id,
            label: m.user?.global_name || m.user?.username || m.member.user_id,
          }))}
          value={user}
          onValueChange={setUser}
        />
        <Select
          aria-label={t('settings.features.temp.role')}
          placeholder={t('settings.features.temp.role')}
          options={roles
            .filter((r) => r.id !== guildId)
            .map((r) => ({ value: r.id, label: r.name }))}
          value={role}
          onValueChange={setRole}
        />
        <Select
          aria-label={t('settings.features.temp.length')}
          options={DURATIONS.map((d) => ({ value: String(d.seconds), label: t(d.key) }))}
          value={String(secs)}
          onValueChange={(v) => setSecs(Number(v))}
        />
        <Button disabled={!user || !role || act.busy} onClick={grant}>
          {t('settings.features.temp.give')}
        </Button>
      </div>
    </section>
  );
}
