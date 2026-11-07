import { useT } from '../i18n';
import './listLoadFailed.css';

/**
 * SEC-AUD-046 (D-577): the row a list shows where it would be when its load failed, with Try again. A load the
 * member did not start never raises a toast.
 */
export function ListLoadFailed({ message, onRetry }: { message: string; onRetry: () => void }) {
  const t = useT();
  return (
    <p role="status" className="lng-list-failed" data-testid="list-load-failed">
      {message}{' '}
      <button type="button" className="lng-list-retry" onClick={onRetry}>
        {t('lounge.listLoad.retry')}
      </button>
    </p>
  );
}
