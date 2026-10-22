// Edit Channel > Modules (LNG-CHN-017): the feature modules that have a per-channel scope. A channel can switch a
// module off, or change one of its channel-level settings, while the server keeps its own choice; "Follow the
// server" drops the channel's overrides. The settings form is the module's own schema narrowed to the channel
// level (crates/sr-features), so a new module with a channel scope needs no screen here. A channel can only narrow
// a module the server has on; it cannot switch on a module the server has off.
import { Button, Notice, Switch } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import {
  SchemaForm,
  overridesOf,
  schemaFields,
  type PickerChoices,
  type SettingsValues,
} from '../../../admin/SchemaForm';
import { UnsavedBar } from '../../../admin/UnsavedBar';
import { useT } from '../../../i18n';
import * as api from '../featuresApi';
import { useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';
import { useFeaturePickers } from './Features';

export function ChannelModules({ guildId, channelId }: { guildId: string; channelId: string }) {
  const t = useT();
  const res = useResource(`channel-modules:${channelId}`, () => api.listChannelFeatures(channelId));
  const pickers = useFeaturePickers(guildId);
  return (
    <div className="ss-form">
      <p className="ss-desc">{t('settings.channelModules.intro')}</p>
      <Async res={res} label={t('settings.channelModules.loading')}>
        {(features) =>
          features.length === 0 ? (
            <p className="ss-na">{t('settings.channelModules.none')}</p>
          ) : (
            <>
              {features.map((f) => (
                <ChannelModuleCard
                  key={f.id}
                  channelId={channelId}
                  feature={f}
                  pickers={pickers}
                  onChanged={(next) =>
                    res.set((cur) => (cur ?? []).map((x) => (x.id === next.id ? next : x)))
                  }
                />
              ))}
            </>
          )
        }
      </Async>
    </div>
  );
}

function ChannelModuleCard({
  channelId,
  feature,
  pickers,
  onChanged,
}: {
  channelId: string;
  feature: api.ChannelFeature;
  pickers: PickerChoices;
  onChanged: (next: api.ChannelFeature) => void;
}) {
  const t = useT();
  const act = useAction();
  // Overrides are measured against what the channel inherits, not the schema default, so a value the server
  // chose stays inherited until the channel really picks something else.
  const fields = useMemo(
    () =>
      schemaFields(feature.schema).map((f) => ({
        ...f,
        default: feature.inherited[f.key],
      })),
    [feature.schema, feature.inherited],
  );
  const [values, setValues] = useState<SettingsValues>(feature.settings);
  const dirty =
    JSON.stringify(overridesOf(fields, values)) !==
    JSON.stringify(overridesOf(fields, feature.settings));
  const id = `chmod-${feature.id.replace(/[^a-z0-9]/gi, '-')}`;
  const hasOverride =
    feature.override_enabled !== null || Object.keys(feature.overrides).length > 0;

  const apply = (next: api.ChannelFeature) => {
    setValues(next.settings);
    onChanged(next);
  };
  const toggle = (enabled: boolean) =>
    void act.run(async () =>
      apply(await api.patchChannelFeature(channelId, feature.id, { enabled })),
    );
  const save = () =>
    void act.run(async () => {
      apply(
        await api.patchChannelFeature(channelId, feature.id, {
          settings: overridesOf(fields, values),
        }),
      );
    }, t('settings.channelModules.saved'));
  const follow = () =>
    void act.run(async () => {
      await api.resetChannelFeature(channelId, feature.id);
      const fresh = (await api.listChannelFeatures(channelId)).find((x) => x.id === feature.id);
      if (fresh) apply(fresh);
    }, t('settings.channelModules.followed'));

  return (
    <article className="ss-card" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="ss-card-title">
        {feature.name}
      </h3>
      <p className="ss-desc">{feature.description}</p>
      {feature.platform_disabled && (
        <Notice variant="warning">{t('settings.features.platformOff')}</Notice>
      )}
      {!feature.guild_enabled && (
        <Notice variant="info">{t('settings.channelModules.serverOff')}</Notice>
      )}
      <Switch
        label={t('settings.channelModules.enable', { name: feature.name })}
        checked={feature.enabled}
        disabled={act.busy || !feature.guild_enabled}
        onChange={(e) => toggle(e.target.checked)}
      />
      <p className="ss-meta">
        {feature.override_enabled === null
          ? t('settings.channelModules.followsServer')
          : t('settings.channelModules.ownSwitch')}
      </p>
      {fields.length > 0 && (
        <div id={`${id}-form`}>
          <SchemaForm fields={fields} values={values} pickers={pickers} onChange={setValues} />
          <UnsavedBar dirty={dirty}>
            <Button size="sm" disabled={!dirty || act.busy} onClick={save}>
              {t('settings.channelModules.save', { name: feature.name })}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={!dirty || act.busy}
              onClick={() => setValues(feature.settings)}
            >
              {t('settings.channelModules.undo', { name: feature.name })}
            </Button>
          </UnsavedBar>
        </div>
      )}
      {hasOverride && (
        <Button size="sm" variant="secondary" disabled={act.busy} onClick={follow}>
          {t('settings.channelModules.follow', { name: feature.name })}
        </Button>
      )}
      <ActionStatus error={act.error} done={act.done} />
    </article>
  );
}
