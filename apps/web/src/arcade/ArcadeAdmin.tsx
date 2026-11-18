import { Button, Notice, Select, TextInput } from '@sideroom/ui';
import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { useT } from '../i18n';
import { SectionHeader } from '../admin/ui';
import { errorText } from '../settings/server/api';
import { httpArcadeAdminApi, type ArcadeAdminApi } from './adminApi';
import { httpArcadeApi, type ArcadeApi } from './api';
import { TIERS, type Candidate, type ComingSoonCard, type Tier } from './types';

const tierKey = (t: Tier) =>
  t === 'TIER_1_ORIGINAL' ? 'arcade.tier.original' : 'arcade.tier.reimplementation';

/** Admin > Arcade (`OPERATOR`): who is featured on the Front Page and the Coming Soon showcase. */
export function ArcadeAdminSection({
  admin = httpArcadeAdminApi,
  api = httpArcadeApi,
}: {
  admin?: ArcadeAdminApi;
  api?: Pick<ArcadeApi, 'front'>;
}) {
  const t = useT();
  const [cands, setCands] = useState<Candidate[] | null>(null);
  const [soon, setSoon] = useState<ComingSoonCard[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [released, setReleased] = useState<string | null>(null);
  const [title, setTitle] = useState('');
  const [summary, setSummary] = useState('');
  const [tier, setTier] = useState<Tier>('TIER_1_ORIGINAL');
  const [repo, setRepo] = useState('');
  // Per Coming Soon entry: the released Activity chosen in "Released as" ('' = none, keep the card's title).
  const [releaseAs, setReleaseAs] = useState<Record<string, string>>({});

  const fail = (e: unknown) => setError(errorText(e, ''));
  const load = useCallback(() => {
    setError(null);
    admin
      .candidates()
      .then(setCands)
      .catch((e: unknown) => setError(errorText(e, '')));
    api
      .front()
      .then((f) => setSoon(f.coming_soon))
      .catch((e: unknown) => setError(errorText(e, '')));
  }, [admin, api]);
  useEffect(load, [load]);

  const release = (s: ComingSoonCard) =>
    admin
      .releaseComingSoon(s.id, ...(releaseAs[s.id] ? [{ application_id: releaseAs[s.id] }] : []))
      .then((r) => {
        setReleased(
          t('arcade.admin.soonReleased', {
            title: s.title,
            told: String(r.notified),
            watchers: String(r.watchers),
          }),
        );
        load();
      })
      .catch(fail);
  const flip = (c: Candidate) =>
    (c.featured ? admin.unfeature(c.application_id) : admin.feature(c.application_id, {}))
      .then(load)
      .catch(fail);
  const add = (e: FormEvent) => {
    e.preventDefault();
    admin
      .addComingSoon({ title, summary, tier, repo_url: repo })
      .then(() => {
        setTitle('');
        setSummary('');
        setRepo('');
        load();
      })
      .catch(fail);
  };

  return (
    <section className="arc-section arc-admin" aria-label={t('arcade.admin.heading')}>
      <SectionHeader title={t('arcade.admin.heading')} description={t('arcade.admin.intro')} />
      {error !== null && (
        <Notice variant="danger" title={t('arcade.admin.failed', { reason: error })} />
      )}
      <h3>{t('arcade.admin.candidates')}</h3>
      <p className="arc-dim">{t('arcade.admin.heroNote')}</p>
      {cands === null && <p className="arc-dim">{t('arcade.admin.loading')}</p>}
      {cands?.length === 0 && <p className="arc-dim">{t('arcade.admin.noCandidates')}</p>}
      <ul className="arc-admin-list">
        {(cands ?? []).map((c) => (
          <li key={c.application_id}>
            <strong>{c.name}</strong> <span className="arc-chip">{t(tierKey(c.tier))}</span>{' '}
            {c.featured && <span className="arc-chip">{t('arcade.admin.featured')}</span>}{' '}
            <Button size="sm" variant="secondary" onClick={() => void flip(c)}>
              {t(c.featured ? 'arcade.admin.unfeature' : 'arcade.admin.feature', { name: c.name })}
            </Button>
          </li>
        ))}
      </ul>
      <h3>{t('arcade.admin.soonHeading')}</h3>
      <p className="arc-dim">{t('arcade.admin.soonReleaseHint')}</p>
      {released !== null && <Notice variant="success" title={released} />}
      <ul className="arc-admin-list">
        {soon.map((s) => (
          <li key={s.id}>
            <strong>{s.title}</strong> <span className="arc-chip">{t(tierKey(s.tier))}</span>
            <form
              className="arc-release"
              onSubmit={(e) => {
                e.preventDefault();
                void release(s);
              }}
            >
              <div className="arc-field">
                <label htmlFor={`arc-release-${s.id}`}>{t('arcade.admin.soonReleaseAs')}</label>
                <Select
                  id={`arc-release-${s.id}`}
                  options={[
                    { value: '', label: t('arcade.admin.soonReleaseAsNone') },
                    ...(cands ?? []).map((c) => ({ value: c.application_id, label: c.name })),
                  ]}
                  value={releaseAs[s.id] ?? ''}
                  onValueChange={(v) => setReleaseAs({ ...releaseAs, [s.id]: v })}
                />
              </div>
              <Button size="sm" type="submit">
                {t('arcade.admin.soonRelease', { title: s.title })}
              </Button>
              <Button
                size="sm"
                variant="secondary"
                type="button"
                onClick={() => void admin.removeComingSoon(s.id).then(load).catch(fail)}
              >
                {t('arcade.admin.soonRemove', { title: s.title })}
              </Button>
            </form>
          </li>
        ))}
      </ul>
      <form className="arc-form" onSubmit={add}>
        <TextInput
          label={t('arcade.admin.soonTitle')}
          value={title}
          maxLength={60}
          onChange={(e) => setTitle(e.target.value)}
        />
        <TextInput
          label={t('arcade.admin.soonSummary')}
          value={summary}
          maxLength={240}
          onChange={(e) => setSummary(e.target.value)}
        />
        <div className="arc-field">
          <label htmlFor="arc-admin-tier">{t('arcade.admin.soonTier')}</label>
          <Select
            id="arc-admin-tier"
            options={TIERS.map((x) => ({ value: x, label: t(tierKey(x)) }))}
            value={tier}
            onValueChange={(v) => setTier(v as Tier)}
          />
        </div>
        <TextInput
          label={t('arcade.admin.soonRepo')}
          value={repo}
          onChange={(e) => setRepo(e.target.value)}
        />
        <Button type="submit" disabled={!title.trim() || !repo.trim()}>
          {t('arcade.admin.soonAdd')}
        </Button>
      </form>
    </section>
  );
}
