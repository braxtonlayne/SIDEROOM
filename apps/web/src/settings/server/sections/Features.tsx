// Server Settings > Features (FTR-*): the built-in feature modules of this server (starboard, welcome messages,
// reaction roles, sticky messages, ...). Every module is a switch plus a settings form the server describes
// itself (crates/sr-features), so a new module needs no screen here. Temporary roles also get a small panel
// to grant and end roles, because they are actions as well as settings.
import { Button, Notice, Switch, TextInput } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import {
  SchemaForm,
  overridesOf,
  schemaFields,
  type PickerChoices,
  type SettingsValues,
} from '../../../admin/SchemaForm';
import { useT } from '../../../i18n';
import { useChannels, useGuild } from '../../../store';
import * as api from '../featuresApi';
import { useResource } from '../hooks';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';
import type { SectionProps } from '../ServerSettings';
import { MessagePicker } from './MessagePicker';
import { UnsavedBar } from '../../../admin/UnsavedBar';
import { FeatureSlot } from './featureSlots';
import { BridgeCard } from '../bridge/BridgeCard';
import { BRIDGE_MODULE_ID, type BridgeFeature } from '../bridge/api';
import { useBridgeModule } from '../bridge/moduleStore';

const REACTION_ROLES_ID = 'lounge.reaction_roles';
/** Channel type code of a category; it holds channels and is not somewhere a module can post. */
const CATEGORY = 4;

/** The channel and role choices a module's pickers offer for this server. */
export function useFeaturePickers(guildId: string): PickerChoices {
  const channels = useChannels(guildId);
  const { roles } = useGuild(guildId);
  return useMemo<PickerChoices>(
    () => ({
      channels: channels
        .filter((c) => c.type !== CATEGORY)
        .map((c) => ({ id: c.id, name: c.name ?? c.id })),
      roles: roles.filter((r) => r.id !== guildId).map((r) => ({ id: r.id, name: r.name })),
    }),
    [channels, roles, guildId],
  );
}

export function FeaturesSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  const setBridgeModule = useBridgeModule((s) => s.set);
  const res = useResource(`features-${guildId}`, async () => {
    const list = await api.listFeatures(guildId);
    // The Discord bridge's nav entry follows what this screen just read (BRG-UI-001).
    const bridge = list.find((f) => f.id === BRIDGE_MODULE_ID);
    if (bridge) setBridgeModule(guildId, bridge.enabled && !bridge.platform_disabled);
    return list;
  });
  const [query, setQuery] = useState('');
  const pickers = useFeaturePickers(guildId);

  return (
    <>
      <SectionHeader
        title={t('settings.features.title')}
        description={t('settings.features.description')}
      />
      <Async res={res} label={t('settings.features.loading')}>
        {(features) => {
          const needle = query.trim().toLowerCase();
          const shown = needle
            ? features.filter((f) =>
                `${f.name} ${f.description} ${f.section}`.toLowerCase().includes(needle),
              )
            : features;
          const bySection = new Map<string, api.GuildFeature[]>();
          for (const f of shown) bySection.set(f.section, [...(bySection.get(f.section) ?? []), f]);
          if (features.length === 0)
            return <p className="ss-meta">{t('settings.features.none')}</p>;
          return (
            <>
              <TextInput
                label={t('settings.features.search')}
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {shown.length === 0 && <p className="ss-meta">{t('settings.features.noMatch')}</p>}
              {[...bySection.entries()].map(([section, list]) => (
                <section key={section} aria-label={section}>
                  <h2 className="ss-sub">{section}</h2>
                  {list.map((f) => {
                    const onChanged = (next: api.GuildFeature) =>
                      res.set((cur) => (cur ?? []).map((x) => (x.id === next.id ? next : x)));
                    // The bridge is the one module only the owner switches (D-381): its own card, with the notice.
                    return f.id === BRIDGE_MODULE_ID ? (
                      <BridgeCard
                        key={f.id}
                        guildId={guildId}
                        feature={f as BridgeFeature}
                        isOwner={viewer.isOwner}
                        onChanged={onChanged}
                      />
                    ) : (
                      <FeatureCard
                        key={f.id}
                        guildId={guildId}
                        feature={f}
                        pickers={pickers}
                        onChanged={onChanged}
                      />
                    );
                  })}
                </section>
              ))}
            </>
          );
        }}
      </Async>
    </>
  );
}

export function FeatureCard({
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
              <UnsavedBar dirty={dirty}>
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
              </UnsavedBar>
            </div>
          )}
        </>
      )}
      <ActionStatus error={act.error} done={act.done} />
      {feature.enabled && <FeatureSlot guildId={guildId} feature={feature} />}
    </article>
  );
}
