import { useT } from '../../i18n';
import { useStore } from '../../store';
import './apps.css';

/**
 * "Only you can see this" with a Dismiss link, shown under an app's ephemeral reply (DEV-BOT-011). The reply
 * is never stored on the server, so dismissing only removes it from this client.
 */
export function EphemeralNote({ channelId, messageId }: { channelId: string; messageId: string }) {
  const t = useT();
  return (
    <p className="lng-ephemeral">
      <span>{t('lounge.apps.ephemeral.note')}</span>
      <button
        type="button"
        className="lng-ephemeral-dismiss"
        onClick={() => useStore.getState().dismissLocalMessage(channelId, messageId)}
      >
        {t('lounge.apps.ephemeral.dismiss')}
      </button>
    </p>
  );
}
