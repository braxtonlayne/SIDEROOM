import { useMemo } from 'react';
import { useT } from '../i18n';
import { useStore } from '../store';
import '../lounge/bridge/bridge.css';

/**
 * The privacy page's list of bridged channels (BRG-UI-001, D-381): every channel of this person's servers that the
 * Discord bridge mirrors, derived from the channels the client has loaded (`sr_bridge` on the channel). It empties
 * when a server switches the bridge off. There is no per-member opt-out. Shown only to a signed-in person (D-385).
 */
export function BridgedChannels() {
  const t = useT();
  const channels = useStore((s) => s.channels);
  const guilds = useStore((s) => s.guilds);
  const rows = useMemo(
    () =>
      Object.values(channels)
        .filter((c) => c.sr_bridge && c.guild_id)
        .map((c) => ({
          id: c.id,
          channel: c.name ?? c.id,
          server: guilds[c.guild_id!]?.name ?? '',
        })),
    [channels, guilds],
  );
  return (
    <div data-testid="bridged-channels">
      <h3>{t('help.bridge.listTitle')}</h3>
      {rows.length === 0 ? (
        <p>{t('help.bridge.listEmpty')}</p>
      ) : (
        <ul className="lng-bridge-list">
          {rows.map((r) => (
            <li key={r.id}>
              {t('help.bridge.listItem', { channel: r.channel, server: r.server })}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
