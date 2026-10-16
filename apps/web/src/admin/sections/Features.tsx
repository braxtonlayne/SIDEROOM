import { Badge, Button, Drawer, Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { SchemaForm, overridesOf, schemaFields, type SettingsValues } from '../SchemaForm';
import { Async, ReasonDialog, SectionHeader, useResource } from '../ui';
import { UnsavedBar } from '../UnsavedBar';

const savedSettings = (f: api.FeatureView): SettingsValues =>
  f.settings && typeof f.settings === 'object' ? (f.settings as SettingsValues) : {};

export function FeaturesSection() {
  const t = useT();
  const res = useResource('admin:features', api.listFeatures);
  const [target, setTarget] = useState<{ f: api.FeatureView; enabled: boolean | 'reset' } | null>(
    null,
  );
  const [notice, setNotice] = useState<string | null>(null);
  const [configuring, setConfiguring] = useState<api.FeatureView | null>(null);
  const [draft, setDraft] = useState<SettingsValues>({});
  const [saving, setSaving] = useState(false);
  const dirty =
    !!configuring &&
    JSON.stringify(overridesOf(schemaFields(configuring.schema), draft)) !==
      JSON.stringify(overridesOf(schemaFields(configuring.schema), savedSettings(configuring)));

  return (
    <>
      <SectionHeader
        title={t('admin.features.title')}
        description={t('admin.features.description')}
      />
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <Async res={res} label={t('admin.features.loading')}>
        {(list) =>
          list.length === 0 ? (
            <p className="ss-na">{t('admin.features.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.features.listLabel')}>
              {list.map((f) => (
                <li key={f.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      {f.name} <Badge>{f.section}</Badge>{' '}
                      {f.enabled !== f.default_enabled && (
                        <Badge variant="warning">{t('admin.features.overridden')}</Badge>
                      )}
                    </div>
                    <div className="ss-meta">{f.description}</div>
                  </div>
                  {schemaFields(f.schema).length > 0 && (
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('admin.features.settingsAria', { name: f.name })}
                      onClick={() => {
                        setDraft(savedSettings(f));
                        setConfiguring(f);
                      }}
                    >
                      {t('admin.features.settings')}
                    </Button>
                  )}
                  <Switch
                    label={t('admin.features.enabledLabel', { name: f.name })}
                    checked={f.enabled}
                    onChange={(e) => setTarget({ f, enabled: e.target.checked })}
                  />
                  {f.enabled !== f.default_enabled && (
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('admin.features.resetAria', { name: f.name })}
                      onClick={() => setTarget({ f, enabled: 'reset' })}
                    >
                      {t('admin.features.reset')}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <Drawer
        open={!!configuring}
        onClose={() => setConfiguring(null)}
        title={t('admin.features.settingsTitle', { name: configuring?.name ?? '' })}
      >
        {configuring && (
          <div className="adm-form">
            <p className="ss-sub">{t('admin.features.settingsIntro')}</p>
            <SchemaForm
              fields={schemaFields(configuring.schema)}
              values={draft}
              onChange={setDraft}
            />
            <UnsavedBar dirty={dirty}>
              <Button loading={saving} disabled={!dirty} onClick={() => setSaving(true)}>
                {t('admin.features.saveSettings')}
              </Button>
              <Button
                variant="secondary"
                disabled={!dirty || saving}
                onClick={() => setDraft(savedSettings(configuring))}
              >
                {t('admin.features.discardChanges')}
              </Button>
            </UnsavedBar>
          </div>
        )}
      </Drawer>
      <ReasonDialog
        open={saving && !!configuring}
        title={t('admin.features.saveSettingsTitle', { name: configuring?.name ?? '' })}
        confirmLabel={t('admin.features.saveSettings')}
        reasonOptional
        onCancel={() => setSaving(false)}
        onConfirm={async (reason) => {
          if (!configuring) return;
          await api.patchFeature(configuring.id, {
            settings: overridesOf(schemaFields(configuring.schema), draft),
            ...(reason ? { reason } : {}),
          });
          setNotice(t('admin.features.settingsSaved', { name: configuring.name }));
          setSaving(false);
          setConfiguring(null);
          await res.reload();
        }}
      />
      <ReasonDialog
        open={!!target}
        title={
          target?.enabled === 'reset'
            ? t('admin.features.resetTitle', { name: target.f.name })
            : t(target?.enabled ? 'admin.features.enableTitle' : 'admin.features.disableTitle', {
                name: target?.f.name ?? '',
              })
        }
        description={
          target?.enabled === false
            ? t('admin.features.disableDescription')
            : target?.enabled === 'reset'
              ? t('admin.features.resetDescription', {
                  state: target.f.default_enabled
                    ? t('admin.features.stateOn')
                    : t('admin.features.stateOff'),
                })
              : undefined
        }
        confirmLabel={
          target?.enabled === 'reset'
            ? t('admin.features.reset')
            : target?.enabled
              ? t('admin.features.enable')
              : t('admin.features.disable')
        }
        danger={target?.enabled === false}
        reasonOptional
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.patchFeature(target.f.id, {
            ...(target.enabled === 'reset' ? { reset_enabled: true } : { enabled: target.enabled }),
            ...(reason ? { reason } : {}),
          });
          setNotice(t('admin.features.updated', { name: target.f.name }));
          setTarget(null);
          await res.reload();
        }}
      />
    </>
  );
}
