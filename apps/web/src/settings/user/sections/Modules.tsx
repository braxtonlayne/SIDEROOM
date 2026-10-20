// User Settings > Modules (LNG-USET-036): the personal settings of feature modules (crates/sr-features, user
// scope). A server decides whether a module runs; the settings listed here are the person's own preferences
// for it and follow them into every server. The form is the module's own schema narrowed to the personal level,
// so a module that gains a user scope shows up here without a new screen.
import { Button } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import {
  SchemaForm,
  overridesOf,
  schemaFields,
  type SettingsValues,
} from '../../../admin/SchemaForm';
import { UnsavedBar } from '../../../admin/UnsavedBar';
import { useT } from '../../../i18n';
import * as fapi from '../../server/featuresApi';
import { errorText, useResource } from '../api';
import { Card, Loaded, SectionTitle } from '../parts';

export function ModulesSection() {
  const t = useT();
  const res = useResource(fapi.listUserFeatures);
  return (
    <>
      <SectionTitle>{t('settings.userModules.title')}</SectionTitle>
      <p className="us-muted">{t('settings.userModules.intro')}</p>
      <Loaded res={res} what={t('settings.userModules.what')}>
        {(features) =>
          features.length === 0 ? (
            <p className="us-muted">{t('settings.userModules.none')}</p>
          ) : (
            <>
              {features.map((f) => (
                <ModuleCard
                  key={f.id}
                  feature={f}
                  onChanged={(next) =>
                    res.setData((cur) => (cur ?? []).map((x) => (x.id === next.id ? next : x)))
                  }
                />
              ))}
            </>
          )
        }
      </Loaded>
    </>
  );
}

function ModuleCard({
  feature,
  onChanged,
}: {
  feature: fapi.UserFeature;
  onChanged: (next: fapi.UserFeature) => void;
}) {
  const t = useT();
  const fields = useMemo(() => schemaFields(feature.schema), [feature.schema]);
  const [values, setValues] = useState<SettingsValues>(feature.settings);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const dirty =
    JSON.stringify(overridesOf(fields, values)) !==
    JSON.stringify(overridesOf(fields, feature.settings));
  const hasOverride = Object.keys(feature.overrides).length > 0;
  const id = `us-module-${feature.id.replace(/[^a-z0-9]/gi, '-')}`;

  const run = async (fn: () => Promise<fapi.UserFeature>, ok: string) => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const next = await fn();
      setValues(next.settings);
      onChanged(next);
      setDone(ok);
    } catch (e) {
      setError(errorText(e, t('settings.userModules.saveError')));
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    void run(
      () => fapi.patchUserFeature(feature.id, overridesOf(fields, values)),
      t('settings.userModules.saved'),
    );
  const reset = () =>
    void run(async () => {
      await fapi.resetUserFeature(feature.id);
      const fresh = (await fapi.listUserFeatures()).find((x) => x.id === feature.id);
      return fresh ?? { ...feature, settings: {}, overrides: {} };
    }, t('settings.userModules.resetDone'));

  return (
    <Card id={id} title={feature.name} description={feature.description}>
      <SchemaForm fields={fields} values={values} onChange={setValues} />
      <UnsavedBar dirty={dirty}>
        <Button size="sm" disabled={!dirty || busy} onClick={save}>
          {t('settings.userModules.save', { name: feature.name })}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          disabled={!dirty || busy}
          onClick={() => setValues(feature.settings)}
        >
          {t('settings.userModules.undo', { name: feature.name })}
        </Button>
      </UnsavedBar>
      {hasOverride && (
        <Button size="sm" variant="secondary" disabled={busy} onClick={reset}>
          {t('settings.userModules.reset', { name: feature.name })}
        </Button>
      )}
      {error && (
        <p className="us-error" role="alert">
          {error}
        </p>
      )}
      {done && !error && (
        <p className="us-muted" role="status">
          {done}
        </p>
      )}
    </Card>
  );
}
