import { useT } from '../../i18n';
import { useStore } from '../../store';
import './bridge.css';

/**
 * The persistent notice in every mapped channel (BRG-UI-001, D-381): says the channel is mirrored to Discord and
 * that messages from Discord carry a DISCORD tag. There is no per-member opt-out (owner decision): everything in a
 * mapped channel is mirrored, and this banner and the notice before the switch tell members so. It reads
 * `sr_bridge` on the channel object the client already has (absent when the channel is not mapped).
 */
export function BridgeBanner({ channelId }: { channelId: string }) {
  const t = useT();
  const direction = useStore((s) => s.channels[channelId]?.sr_bridge?.direction);
  const whole = useStore((s) => s.channels[channelId]?.sr_bridge?.mode === 'whole_guild');
  if (!direction) return null;
  const text = whole
    ? t('lounge.bridgeSync.bannerWhole')
    : direction === 'both'
      ? t('lounge.bridge.bannerBoth')
      : direction === 'to_discord'
        ? t('lounge.bridge.bannerOut')
        : t('lounge.bridge.bannerIn');
  return (
    <div className="lng-bridge-banner" role="note" data-testid="bridge-banner">
      <p>{text}</p>
    </div>
  );
}
