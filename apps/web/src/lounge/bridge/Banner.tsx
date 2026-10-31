import { Button } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { mirrorsOutbound, useBridge, useBridgedChannel } from './store';
import './bridge.css';

/**
 * The persistent notice in every mapped channel (BRG-UI-001, D-381): says the channel is mirrored to a Discord
 * server and that messages from Discord carry a DISCORD tag, and offers the member's own opt-out. It shows only
 * where the bridge is on for this channel, so nobody is surprised that their words leave the server.
 */
export function BridgeBanner({ channelId }: { channelId: string }) {
  const t = useT();
  const mapped = useBridgedChannel(channelId);
  const optOut = useBridge((s) => s.optOut);
  const [failed, setFailed] = useState(false);
  if (!mapped) return null;

  const name = mapped.discord_guild_name;
  const text =
    mapped.direction === 'both'
      ? t('lounge.bridge.bannerBoth', { name })
      : mapped.direction === 'to_discord'
        ? t('lounge.bridge.bannerOut', { name })
        : t('lounge.bridge.bannerIn', { name });
  const toggle = () => {
    setFailed(false);
    optOut(channelId, !mapped.opted_out).catch(() => setFailed(true));
  };

  return (
    <div className="lng-bridge-banner" role="note" data-testid="bridge-banner">
      <p>
        {text}
        {mirrorsOutbound(mapped.direction) && mapped.opted_out && (
          <> {t('lounge.bridge.bannerOptedOut')}</>
        )}
      </p>
      {mirrorsOutbound(mapped.direction) && (
        <Button size="sm" variant="ghost" onClick={toggle}>
          {mapped.opted_out ? t('lounge.bridge.optIn') : t('lounge.bridge.optOut')}
        </Button>
      )}
      {failed && (
        <span role="alert" className="lng-bridge-err">
          {t('lounge.bridge.optError')}
        </span>
      )}
    </div>
  );
}
