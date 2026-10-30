import { useStore } from '../../store';
import { useT } from '../../i18n';
import type { TvPool } from './types';

/**
 * The chip on a channel that airs from shelves instead of the instance catalog (BYOS-TV-001): "Server shelf"
 * for a server's channel (the same guide for everyone who may watch), "Your shelf" for a personal one.
 * Nothing for the instance catalog's channels.
 */
export function PoolChip({ pool }: { pool: TvPool | null | undefined }) {
  const t = useT();
  const guildId = pool?.kind === 'guild' ? pool.guild_id : undefined;
  const server = useStore((s) => (guildId ? s.guilds[guildId]?.name : undefined));
  if (!pool || pool.kind === 'instance') return null;
  if (pool.kind === 'personal') {
    return (
      <span className="tv-pool-chip" title={t('cinema.tv.chip.personalTitle')}>
        {t('cinema.tv.chip.personal')}
      </span>
    );
  }
  return (
    <span
      className="tv-pool-chip"
      title={
        server
          ? t('cinema.tv.chip.serverShelfTitle', { server })
          : t('cinema.tv.chip.serverShelfUnnamed')
      }
    >
      {t('cinema.tv.chip.serverShelf')}
    </span>
  );
}
