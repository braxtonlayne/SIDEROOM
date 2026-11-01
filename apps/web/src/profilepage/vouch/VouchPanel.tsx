// The vouch panel of a profile (MOD-VCH-001): the count, the tier badge, the Vouch / Withdraw button and the
// tooltip that documents the accountability multiplier, so nobody farms vouches casually. The tooltip says what
// the tiers are and what a higher tier costs when its holder breaks a rule.
import { Button, Tooltip } from '@sideroom/ui';
import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { httpVouchApi, type VouchApi, type VouchSummary } from './api';
import './vouch.css';

export function VouchPanel({ userId, api = httpVouchApi }: { userId: string; api?: VouchApi }) {
  const t = useT();
  const [sum, setSum] = useState<VouchSummary | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<'failed' | 'tooMany' | null>(null);

  useEffect(() => {
    let dead = false;
    setSum(null);
    setFailed(false);
    setProblem(null);
    api
      .get(userId)
      .then((s) => !dead && setSum(s))
      .catch(() => !dead && setFailed(true));
    return () => {
      dead = true;
    };
  }, [api, userId]);

  const act = useCallback(
    async (revoke: boolean) => {
      setBusy(true);
      setProblem(null);
      try {
        if (revoke) await api.revoke(userId);
        else await api.vouch(userId);
        setSum(await api.get(userId));
      } catch (e) {
        setProblem(e instanceof ApiError && e.status === 409 ? 'tooMany' : 'failed');
      } finally {
        setBusy(false);
      }
    },
    [api, userId],
  );

  if (failed) return <p className="vouch-hint">{t('profile.vouch.loadFailed')}</p>;
  if (!sum) return null;

  const tierLabel = t(`profile.vouch.tier.${sum.tier}`);
  return (
    <section className="vouch-panel" aria-label={t('profile.vouch.region')}>
      <Tooltip
        placement="bottom"
        content={
          <span className="vouch-tip">
            <span>{t('profile.vouch.tooltipLead')}</span>
            <span>
              {t('profile.vouch.tooltipTiers', {
                pillar: sum.pillar_min,
                elderBelow: sum.elder_min - 1,
                elder: sum.elder_min,
              })}
            </span>
            <span>
              {t('profile.vouch.tooltipRule', {
                pillarX: sum.pillar_multiplier,
                elderX: sum.elder_multiplier,
              })}
            </span>
            <span>
              {sum.tier === 'citizen'
                ? t('profile.vouch.tooltipNowPlain')
                : t('profile.vouch.tooltipNow', {
                    tier: tierLabel,
                    count: sum.count,
                    mult: sum.multiplier,
                  })}
            </span>
            <span>{t('profile.vouch.tooltipFallout')}</span>
          </span>
        }
      >
        <button
          type="button"
          className="vouch-badge"
          data-tier={sum.tier}
          aria-label={t('profile.vouch.badge', { tier: tierLabel })}
        >
          {tierLabel}
        </button>
      </Tooltip>
      <span className="vouch-count">{t('profile.vouch.count', { n: sum.count })}</span>
      {!sum.is_self && sum.viewer_vouched && (
        <>
          <span className="vouch-hint">{t('profile.vouch.you')}</span>
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => void act(true)}>
            {t('profile.vouch.revoke')}
          </Button>
        </>
      )}
      {!sum.is_self && !sum.viewer_vouched && sum.can_vouch && (
        <Button size="sm" disabled={busy} onClick={() => void act(false)}>
          {t('profile.vouch.vouch')}
        </Button>
      )}
      {!sum.is_self && !sum.viewer_vouched && !sum.can_vouch && (
        <span className="vouch-hint">{t('profile.vouch.cannot')}</span>
      )}
      {problem && (
        <p role="alert" className="vouch-hint">
          {t(problem === 'tooMany' ? 'profile.vouch.tooMany' : 'profile.vouch.failed')}
        </p>
      )}
    </section>
  );
}
