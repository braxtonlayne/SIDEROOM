// Rank card and leaderboard (FTR-LVL-001): `/rank` and the Community dialog open this. Opting out deletes the
// person's XP, so it asks first. The level roles come with the rank card, so every member sees them (G-70).
import { Avatar, Button, ProgressBar } from '@sideroom/ui';
import { useState } from 'react';
import { formatNumber, useT } from '../../i18n';
import { useGuild } from '../../store';
import { useCurrentUser } from '../data';
import {
  getLeaderboard,
  getRank,
  setLevelingEnabled,
  type LeaderboardEntry,
} from '../features/api';
import { useLoad } from './useLoad';

const PAGE = 25;

export function Leveling({ guildId }: { guildId: string }) {
  const t = useT();
  const me = useCurrentUser();
  const { roles } = useGuild(guildId);
  const rank = useLoad(() => getRank(guildId), `rank-${guildId}`);
  const [rows, setRows] = useState<LeaderboardEntry[]>([]);
  const [more, setMore] = useState(true);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [confirmOut, setConfirmOut] = useState(false);
  const first = useLoad(async () => {
    const list = await getLeaderboard(guildId, 0, PAGE);
    setRows(list);
    setMore(list.length === PAGE);
    return list;
  }, `board-${guildId}`);

  const loadMore = async () => {
    setBusy(true);
    try {
      const list = await getLeaderboard(guildId, rows.length, PAGE);
      setRows((r) => [...r, ...list]);
      setMore(list.length === PAGE);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (enabled: boolean) => {
    setBusy(true);
    setFailed(false);
    try {
      await setLevelingEnabled(guildId, enabled);
      setConfirmOut(false);
      rank.reload();
      first.reload();
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  if (rank.error && first.error)
    return (
      <p className="lng-failed" role="alert">
        {t('lounge.community.leveling.error')}
      </p>
    );
  const card = rank.data;
  // G-70: every member sees the level roles, from the rank card; lowest level first.
  const rewards = [...(card?.level_roles ?? [])].sort((x, y) => x.level - y.level);
  return (
    <div className="lng-comm-stack">
      {card && (
        <section className="lng-rank-card" aria-label={t('lounge.community.leveling.cardLabel')}>
          <Avatar name={card.display_name ?? card.username} size={56} />
          <div className="lng-rank-main">
            <strong>{card.display_name ?? card.username}</strong>
            <span className="lng-muted">
              {card.rank
                ? t('lounge.community.leveling.summary', {
                    level: card.level,
                    rank: card.rank,
                    xp: formatNumber(card.xp),
                  })
                : t('lounge.community.leveling.unranked', { level: card.level })}
            </span>
            <ProgressBar
              label={t('lounge.community.leveling.progress', { level: card.level + 1 })}
              value={card.level_xp}
              max={Math.max(card.level_span, 1)}
            />
            <span className="lng-muted">
              {t('lounge.community.leveling.toNext', {
                xp: formatNumber(Math.max(card.level_span - card.level_xp, 0)),
                messages: formatNumber(card.messages),
              })}
            </span>
          </div>
        </section>
      )}
      {card?.opted_out ? (
        <div>
          <p className="lng-muted">{t('lounge.community.leveling.optedOut')}</p>
          <Button size="sm" disabled={busy} onClick={() => void toggle(true)}>
            {t('lounge.community.leveling.optIn')}
          </Button>
        </div>
      ) : card ? (
        confirmOut ? (
          <div role="group" aria-label={t('lounge.community.leveling.optOutConfirmLabel')}>
            <p className="lng-muted">{t('lounge.community.leveling.optOutWarning')}</p>
            <Button size="sm" variant="danger" disabled={busy} onClick={() => void toggle(false)}>
              {t('lounge.community.leveling.optOutYes')}
            </Button>{' '}
            <Button size="sm" variant="secondary" onClick={() => setConfirmOut(false)}>
              {t('lounge.community.leveling.optOutNo')}
            </Button>
          </div>
        ) : (
          <Button size="sm" variant="secondary" onClick={() => setConfirmOut(true)}>
            {t('lounge.community.leveling.optOut')}
          </Button>
        )
      ) : null}
      {rewards.length > 0 && (
        <section aria-label={t('lounge.community.leveling.rewards')}>
          <h3 className="lng-comm-h">{t('lounge.community.leveling.rewards')}</h3>
          <ul className="lng-rewards">
            {rewards.map((r) => (
              <li key={`${r.level}-${r.role_id}`}>
                {t('lounge.community.leveling.reward', {
                  level: r.level,
                  role: roles.find((x) => x.id === r.role_id)?.name ?? r.role_id,
                })}
                {card && card.level >= r.level ? ` ${t('lounge.community.leveling.earned')}` : ''}
              </li>
            ))}
          </ul>
        </section>
      )}
      <h3 className="lng-comm-h">{t('lounge.community.leveling.leaderboard')}</h3>
      {first.loading ? (
        <p className="lng-muted" role="status">
          {t('lounge.community.loading')}
        </p>
      ) : rows.length === 0 ? (
        <p className="lng-muted">{t('lounge.community.leveling.empty')}</p>
      ) : (
        <ol className="lng-cboard" aria-label={t('lounge.community.leveling.leaderboard')}>
          {rows.map((r) => (
            <li key={r.user_id} className={r.user_id === me.id ? 'lng-cboard-me' : undefined}>
              <span className="lng-cboard-rank">{r.rank}</span>
              <Avatar name={r.display_name ?? r.username} size={32} />
              <span className="lng-cboard-name">{r.display_name ?? r.username}</span>
              <span className="lng-muted">
                {t('lounge.community.leveling.row', { level: r.level, xp: formatNumber(r.xp) })}
              </span>
            </li>
          ))}
        </ol>
      )}
      {more && !first.loading && rows.length > 0 && (
        <Button size="sm" variant="secondary" disabled={busy} onClick={() => void loadMore()}>
          {t('lounge.community.leveling.more')}
        </Button>
      )}
      {failed && (
        <p className="lng-failed" role="alert">
          {t('lounge.community.leveling.error')}
        </p>
      )}
    </div>
  );
}
