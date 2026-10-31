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
          guild: c.guild_id!,
          whole: c.sr_bridge?.mode === 'whole_guild',
        })),
    [channels, guilds],
  );
  // A server bridged whole gets one plain sentence instead of a list of every channel.
  const wholeServers = [...new Set(rows.filter((r) => r.whole).map((r) => r.guild))];
  const listed = rows.filter((r) => !r.whole);
  return (
    <div data-testid="bridged-channels">
      <h3>{t('help.bridge.listTitle')}</h3>
      {wholeServers.map((g) => (
        <p key={g} data-testid="bridged-whole">
          {t('help.bridgeSync.wholeNotice', { server: guilds[g]?.name ?? '' })}
        </p>
      ))}
      {rows.length === 0 ? (
        <p>{t('help.bridge.listEmpty')}</p>
      ) : listed.length > 0 ? (
        <ul className="lng-bridge-list">
          {listed.map((r) => (
            <li key={r.id}>
              {t('help.bridge.listItem', { channel: r.channel, server: r.server })}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
