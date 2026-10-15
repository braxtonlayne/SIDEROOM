import { useToast } from '@sideroom/ui';
import { useT } from '../../i18n';
import type { Outcome } from './interactions';

/** Report a failed interaction the same way everywhere (a toast); successes are silent. */
export function useReportOutcome() {
  const t = useT();
  const toast = useToast();
  return (o: Outcome) => {
    if (o.ok) return;
    toast.show({
      title:
        o.reason === 'rejected' && o.message
          ? t('lounge.apps.rejected', { message: o.message })
          : t('lounge.components.interactionFailed'),
      variant: 'danger',
      duration: 4000,
    });
  };
}
