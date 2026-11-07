import { useToast } from '@sideroom/ui';
import { useEffect } from 'react';
import { useT } from '../../i18n';
import { onActionError } from '../actionError';

/** Shows a failed member action (reported through `reportActionError`) as a danger toast. Renders nothing. */
export function ActionErrorToasts(): null {
  const t = useT();
  const toast = useToast();
  useEffect(
    () => onActionError((key) => toast.show({ title: t(key), variant: 'danger', duration: 5000 })),
    [t, toast],
  );
  return null;
}
