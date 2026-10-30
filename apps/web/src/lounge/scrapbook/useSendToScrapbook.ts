import { useMemo } from 'react';
import type { MenuEntry } from '@sideroom/ui';
import { useT } from '../../i18n';
import { sendToScrapbook } from './api';
import { useGuildScrapbook } from './store';

/**
 * The "Send to Scrapbook" entry of the message menu, or `null` when the viewer cannot use it: the operator has
 * not offered the module, the server has it off, or the viewer holds neither a permitted role nor Manage
 * Messages. The server decides again on the request.
 */
export function useScrapbookMenuEntry(
  guildId: string | undefined,
  channelId: string,
  messageId: string,
  pending: boolean,
  onResult?: (ok: boolean) => void,
): MenuEntry | null {
  const t = useT();
  const status = useGuildScrapbook(guildId && guildId !== '@me' ? guildId : undefined);
  const allowed = !!status && status.offered && status.enabled && status.can_send && !pending;
  return useMemo(
    () =>
      allowed
        ? {
            label: t('lounge.scrapbook.send'),
            onSelect: () =>
              void sendToScrapbook(channelId, messageId)
                .then(() => onResult?.(true))
                .catch(() => onResult?.(false)),
          }
        : null,
    [allowed, t, channelId, messageId, onResult],
  );
}
