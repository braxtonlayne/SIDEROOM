import type { MenuEntry } from '@sideroom/ui';
import { useT } from '../../i18n';
import { mirrorsOutbound, useBridge } from './store';

/**
 * The member's own "Don't mirror my messages" entry in a mapped channel's menu (BRG-UI-001, D-381): their
 * messages then stay here only. Empty for a channel the bridge does not mirror, or one that only brings Discord
 * messages in (there is nothing of theirs to keep back).
 */
export function useBridgeMenuEntries(
  channelId: string,
  onError: (message: string) => void,
): MenuEntry[] {
  const t = useT();
  const mapped = useBridge((s) => s.channels.find((c) => c.channel_id === channelId));
  const optOut = useBridge((s) => s.optOut);
  if (!mapped || !mirrorsOutbound(mapped.direction)) return [];
  const out = mapped.opted_out;
  return [
    { type: 'separator' },
    {
      label: out ? t('lounge.bridge.optIn') : t('lounge.bridge.optOut'),
      onSelect: () =>
        void optOut(channelId, !out).catch(() => onError(t('lounge.bridge.optError'))),
    },
  ];
}
