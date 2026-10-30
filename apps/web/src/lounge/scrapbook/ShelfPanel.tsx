import { useCallback, useEffect, useState } from 'react';
import { Button } from '@sideroom/ui';
import { useT } from '../../i18n';
import {
  clearTarget,
  dismissRemoval,
  listGuildMounts,
  listRemovals,
  retryRemoval,
  setTarget,
  type GuildMount,
  type ScrapbookRemoval,
} from './api';
import { useGuildScrapbook, useScrapbookStatus } from './store';

/**
 * For the server's managers: the shelf the scrapbook writes to, and the copies that could not be removed from
 * a bucket. The cascade on message delete runs against a bucket the server (or its member) owns; when that
 * bucket refuses, nothing is hidden here: each failed removal is listed with its error and its next try.
 */
export function RemovalList({
  guildId,
  removals,
  onChanged,
}: {
  guildId: string;
  removals: ScrapbookRemoval[];
  onChanged: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState<string | null>(null);
  const act = async (id: string, fn: () => Promise<unknown>) => {
    setBusy(id);
    try {
      await fn();
    } finally {
      setBusy(null);
      onChanged();
    }
  };
  if (removals.length === 0)
    return (
      <p className="lng-scr-note" data-testid="scr-removals-none">
        {t('lounge.scrapbook.removalsNone')}
      </p>
    );
  return (
    <ul className="lng-scr-removals" data-testid="scr-removals">
      {removals.map((r) => (
        <li key={r.id}>
          <p className="lng-scr-removal-key">{r.key}</p>
          <p className="lng-scr-note">
            {t('lounge.scrapbook.removalFailed', { n: r.attempts, error: r.last_error })}
          </p>
          <div className="lng-scr-actions">
            <Button
              variant="secondary"
              disabled={busy === r.id}
              onClick={() => void act(r.id, () => retryRemoval(guildId, r.id))}
            >
              {t('lounge.scrapbook.retry')}
            </Button>
            <Button
              variant="ghost"
              disabled={busy === r.id}
              onClick={() => void act(r.id, () => dismissRemoval(guildId, r.id))}
            >
              {t('lounge.scrapbook.dismiss')}
            </Button>
          </div>
        </li>
      ))}
    </ul>
  );
}

export function ShelfPanel({ guildId }: { guildId: string }) {
  const t = useT();
  const status = useGuildScrapbook(guildId);
  const refresh = useScrapbookStatus((s) => s.refresh);
  const [mounts, setMounts] = useState<GuildMount[]>([]);
  const [removals, setRemovals] = useState<ScrapbookRemoval[]>([]);
  const [pick, setPick] = useState('');
  const [error, setError] = useState(false);

  const reload = useCallback(async () => {
    try {
      const r = await listRemovals(guildId);
      setRemovals(r.removals);
    } catch {
      /* the status already says what it can */
    }
    void refresh(guildId);
  }, [guildId, refresh]);

  useEffect(() => {
    void listGuildMounts(guildId)
      .then(setMounts)
      .catch(() => setMounts([]));
    void reload();
  }, [guildId, reload]);

  if (!status || !status.can_manage) return null;
  const target = status.target;

  const choose = async () => {
    setError(false);
    try {
      await setTarget(guildId, pick);
      void reload();
    } catch {
      setError(true);
    }
  };

  return (
    <section className="lng-scr-shelf" aria-label={t('lounge.scrapbook.shelf')}>
      <h3>{t('lounge.scrapbook.shelf')}</h3>
      {target ? (
        <p className="lng-scr-note" data-testid="scr-target">
          {target.ok
            ? t('lounge.scrapbook.shelfOk', { label: target.label || target.mount_id })
            : t('lounge.scrapbook.shelfProblem', {
                label: target.label || target.mount_id,
                problem: target.problem ?? '',
              })}
        </p>
      ) : (
        <p className="lng-scr-note" role="status" data-testid="scr-no-target">
          {t('lounge.scrapbook.shelfNone')}
        </p>
      )}
      <div className="lng-scr-actions">
        <select
          className="lng-scr-input"
          aria-label={t('lounge.scrapbook.shelfPick')}
          value={pick}
          onChange={(e) => setPick(e.target.value)}
        >
          <option value="">{t('lounge.scrapbook.shelfPick')}</option>
          {mounts.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
        <Button onClick={() => void choose()} disabled={!pick}>
          {t('lounge.scrapbook.shelfSet')}
        </Button>
        {target ? (
          <Button variant="ghost" onClick={() => void clearTarget(guildId).then(() => reload())}>
            {t('lounge.scrapbook.shelfClear')}
          </Button>
        ) : null}
      </div>
      <p className="lng-scr-note">{t('lounge.scrapbook.shelfHelp')}</p>
      {error ? (
        <p className="lng-scr-note" role="alert">
          {t('lounge.scrapbook.shelfFailed')}
        </p>
      ) : null}
      <h3>{t('lounge.scrapbook.removalsTitle')}</h3>
      <RemovalList guildId={guildId} removals={removals} onChanged={() => void reload()} />
    </section>
  );
}
