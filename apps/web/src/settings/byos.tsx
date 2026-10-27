import { Notice } from '@sideroom/ui';
import { useT } from '../i18n';
import { useClientFlags } from '../moderation/reports';

/** The BYOS kill switch (`platform.byos`, BYOS-FLG-001). Only an explicit `false` counts: before the flags load the screens show. */
export function useByosOn(): boolean {
  return useClientFlags((s) => s.flags['platform.byos'] !== false);
}

/** Shown in place of a storage screen while the operator has switched bring-your-own-storage off. */
export function ByosOffNotice() {
  const t = useT();
  return (
    <Notice variant="info" title={t('settings.byosOff.title')}>
      {t('settings.byosOff.body')}
    </Notice>
  );
}
