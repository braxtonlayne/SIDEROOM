import { Button } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../i18n';
import { mirrorsOutbound, useBridge } from '../lounge/bridge/store';
import '../lounge/bridge/bridge.css';

/**
 * The privacy page's list of bridged channels (BRG-UI-001, D-381): every channel of this person's servers that the
 * Discord bridge mirrors, with the Discord server it goes to and the member's own opt-out. It empties when a server
 * switches the bridge off. Shown only to a signed-in person (the handbook is behind sign-in, D-385).
 */
export function BridgedChannels() {
  const t = useT();
  const channels = useBridge((s) => s.channels);
  const load = useBridge((s) => s.load);
  const optOut = useBridge((s) => s.optOut);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    // Always fresh: this list is the privacy statement of what leaves a server.
    void load(true);
  }, [load]);

  return (
    <div data-testid="bridged-channels">
      <h3>{t('help.bridge.listTitle')}</h3>
      {channels.length === 0 ? (
        <p>{t('help.bridge.listEmpty')}</p>
      ) : (
        <ul className="lng-bridge-list">
          {channels.map((c) => (
            <li key={c.channel_id}>
              <span>
                {t('help.bridge.listItem', {
                  channel: c.channel_name,
                  server: c.guild_name,
                  discord: c.discord_guild_name,
                })}
              </span>
              {mirrorsOutbound(c.direction) && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    setFailed(null);
                    optOut(c.channel_id, !c.opted_out).catch(() => setFailed(c.channel_id));
                  }}
                >
                  {c.opted_out
                    ? t('help.bridge.optIn', { channel: c.channel_name })
                    : t('help.bridge.optOut', { channel: c.channel_name })}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {failed && <p role="alert">{t('lounge.bridge.optError')}</p>}
    </div>
  );
}
