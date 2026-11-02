import type { SettingsLayoutLabels } from '@sideroom/ui';
import { useMemo } from 'react';
import { useT } from '../i18n';

/** The translated words of SettingsLayout's close button and unsaved-changes bar. */
export function useSettingsLayoutLabels(): SettingsLayoutLabels {
  const t = useT();
  return useMemo(
    () => ({
      close: t('settings.layout.close'),
      closeTarget: t('settings.layout.closeTarget'),
      unsaved: t('settings.layout.unsaved'),
      reset: t('settings.layout.reset'),
      save: t('settings.layout.save'),
      saving: t('settings.layout.saving'),
    }),
    [t],
  );
}
