import { ApiError } from '../../../api/client';
import { t } from '../../../i18n';
import { errorText } from '../api';

/**
 * Friendly text for the stable BYOS error codes (byos-core): 409 MOUNT_CAP_REACHED, MOUNT_LABEL_TAKEN and
 * CORS_NOT_READY; 403 (no `manage` grant) and 404 (not yours, or gone). Anything else falls back to errorText.
 */
export function storageErrorText(e: unknown): string {
  if (e instanceof ApiError) {
    const code = String(e.body?.code ?? '');
    if (e.status === 409) {
      if (code === 'MOUNT_CAP_REACHED') return t('settings.storageError.capReached');
      if (code === 'MOUNT_LABEL_TAKEN') return t('settings.storageError.labelTaken');
      if (code === 'CORS_NOT_READY') return t('settings.storageError.corsNotReady');
    }
    if (e.status === 403) return t('settings.storageError.forbidden');
    if (e.status === 404) return t('settings.storageError.notFound');
  }
  return errorText(e);
}
