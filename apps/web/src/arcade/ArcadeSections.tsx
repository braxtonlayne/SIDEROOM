import { Button, Notice, TextInput } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useEffect, useState, type FormEvent } from 'react';
import { useT } from '../i18n';
import { errorText } from '../settings/server/api';
import type { Guild } from '../store/types';
import type { ArcadeApi } from './api';
import { GameCard } from './GameCard';
import { TIERS, type Cartridge, type ComingSoonCard, type FeaturedCard, type Tier } from './types';
import type { CapabilityFlag } from './types';

const tierKey = (t: Tier) =>
  t === 'TIER_1_ORIGINAL' ? 'arcade.tier.original' : 'arcade.tier.reimplementation';

/** Featured / Front Page: the Operator's picks, with the Emulator as a built-in card. */
export function FeaturedSection({
  featured,
  flags,
  emulatorOn,
}: {
  featured: FeaturedCard[];
  flags: CapabilityFlag[];
  emulatorOn: boolean;
}) {
  const t = useT();
  return (
    <section aria-labelledby="arc-featured" id="featured" className="arc-section">
      <h2 id="arc-featured">{t('arcade.featured.heading')}</h2>
      <div className="arc-grid">
        {emulatorOn && (
          <GameCard
            title={t('arcade.emulator.name')}
            description={t('arcade.emulator.description')}
          >
            <Link to="/games/emulator" className="arc-link">
              {t('arcade.emulator.open')}
            </Link>
          </GameCard>
        )}
        {featured.map((f) => (
          <GameCard
            key={f.application_id}
            title={f.name}
            description={f.description}
            blurb={f.blurb}
            tier={f.tier}
            eligibility={f.eligibility}
            capabilities={f.capabilities}
            flags={flags}
            tips={f.tip_links}
            trust={{ applicationId: f.application_id, name: f.name, bundleSha256: f.bundle_sha256 }}
          >
            {f.eligibility.browser_play && (
              <p className="arc-dim">{t('arcade.featured.startHint')}</p>
            )}
          </GameCard>
        ))}
      </div>
      {featured.length === 0 && <p className="arc-dim">{t('arcade.featured.empty')}</p>}
    </section>
  );
}

/** Coming Soon showcase with the per-member Notify me toggle. */
export function ComingSoonSection({
  entries,
  api,
}: {
  entries: ComingSoonCard[];
  api: Pick<ArcadeApi, 'setNotify'>;
}) {
  const t = useT();
  const [notify, setNotify] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(entries.map((e) => [e.id, e.notify])),
  );
  const [failed, setFailed] = useState(false);
  const toggle = (id: string) => {
    const next = !notify[id];
    setNotify((m) => ({ ...m, [id]: next }));
    setFailed(false);
    api.setNotify(id, next).catch(() => {
      setNotify((m) => ({ ...m, [id]: !next }));
      setFailed(true);
    });
  };
  return (
    <section aria-labelledby="arc-soon" id="coming-soon" className="arc-section">
      <h2 id="arc-soon">{t('arcade.soon.heading')}</h2>
      {failed && <Notice variant="danger" title={t('arcade.soon.notifyFailed')} />}
      <div className="arc-grid">
        {entries.map((e) => (
          <GameCard
            key={e.id}
            title={e.title}
            description={e.summary}
            tier={e.tier}
            eligibility={{
              browser_play: e.tier === 'TIER_1_ORIGINAL',
              guest_links: e.tier === 'TIER_1_ORIGINAL',
              featured: true,
              desktop_only: e.tier === 'TIER_2_REIMPLEMENTATION',
              needs_base_game_hint: e.tier === 'TIER_2_REIMPLEMENTATION',
            }}
          >
            <a href={e.repo_url} target="_blank" rel="noopener noreferrer" className="arc-link">
              {t('arcade.soon.repo')}
            </a>
            <Button
              size="sm"
              variant={notify[e.id] ? 'primary' : 'secondary'}
              aria-pressed={!!notify[e.id]}
              onClick={() => toggle(e.id)}
            >
              {t('arcade.soon.notify')}
            </Button>
            {notify[e.id] && <p className="arc-dim">{t('arcade.soon.notifying')}</p>}
          </GameCard>
        ))}
      </div>
      {entries.length === 0 && <p className="arc-dim">{t('arcade.soon.empty')}</p>}
    </section>
  );
}

/** Guild Cartridges (unlisted): a server picker, its cartridges and the form that lists one. */
export function CartridgesSection({
  guilds,
  flags,
  api,
}: {
  guilds: Pick<Guild, 'id' | 'name'>[];
  flags: CapabilityFlag[];
  api: Pick<ArcadeApi, 'cartridges' | 'addCartridge' | 'removeCartridge'>;
}) {
  const t = useT();
  const [guildId, setGuildId] = useState(guilds[0]?.id ?? '');
  const [items, setItems] = useState<Cartridge[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [kind, setKind] = useState<'game' | 'mod'>('game');
  const [tier, setTier] = useState<Tier>('TIER_1_ORIGINAL');
  const [source, setSource] = useState('');

  useEffect(() => {
    if (!guildId) return;
    let live = true;
    setItems(null);
    setLoadFailed(false);
    api
      .cartridges(guildId)
      .then((c) => live && setItems(c))
      .catch(() => live && setLoadFailed(true));
    return () => {
      live = false;
    };
  }, [api, guildId]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    api
      .addCartridge(guildId, {
        title,
        description,
        kind,
        tier,
        source_ref: source,
        capabilities: [],
        tip_links: [],
      })
      .then((c) => {
        setItems((cur) => [c, ...(cur ?? [])]);
        setTitle('');
        setDescription('');
        setSource('');
      })
      .catch((err: unknown) => setError(errorText(err, '')));
  };
  const remove = (id: string) => {
    api
      .removeCartridge(guildId, id)
      .then(() => setItems((cur) => (cur ?? []).filter((c) => c.id !== id)))
      .catch((err: unknown) => setError(errorText(err, '')));
  };

  return (
    <section aria-labelledby="arc-carts" id="cartridges" className="arc-section">
      <h2 id="arc-carts">{t('arcade.cartridges.heading')}</h2>
      <p className="arc-dim">{t('arcade.cartridges.intro')}</p>
      {guilds.length === 0 ? (
        <p className="arc-dim">{t('arcade.cartridges.none')}</p>
      ) : (
        <>
          <label className="arc-field">
            <span>{t('arcade.cartridges.server')}</span>
            <select value={guildId} onChange={(e) => setGuildId(e.target.value)}>
              {guilds.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </label>
          {loadFailed && <Notice variant="danger" title={t('arcade.cartridges.failed')} />}
          <div className="arc-grid">
            {(items ?? []).map((c) => (
              <GameCard
                key={c.id}
                title={c.title}
                description={c.description}
                badge={t(
                  c.kind === 'mod' ? 'arcade.cartridges.kindMod' : 'arcade.cartridges.kindGame',
                )}
                tier={c.tier}
                eligibility={c.eligibility}
                capabilities={c.capabilities}
                flags={flags}
                tips={c.tip_links}
              >
                <Button size="sm" variant="secondary" onClick={() => remove(c.id)}>
                  {t('arcade.cartridges.remove', { title: c.title })}
                </Button>
              </GameCard>
            ))}
          </div>
          {items?.length === 0 && <p className="arc-dim">{t('arcade.cartridges.empty')}</p>}
          <form className="arc-form" onSubmit={submit}>
            <h3>{t('arcade.cartridges.addHeading')}</h3>
            <p className="arc-dim">{t('arcade.cartridges.ipRule')}</p>
            <TextInput
              label={t('arcade.cartridges.title')}
              value={title}
              maxLength={60}
              onChange={(e) => setTitle(e.target.value)}
            />
            <TextInput
              label={t('arcade.cartridges.description')}
              value={description}
              maxLength={240}
              onChange={(e) => setDescription(e.target.value)}
            />
            <label className="arc-field">
              <span>{t('arcade.cartridges.kind')}</span>
              <select value={kind} onChange={(e) => setKind(e.target.value as 'game' | 'mod')}>
                <option value="game">{t('arcade.cartridges.kindGame')}</option>
                <option value="mod">{t('arcade.cartridges.kindMod')}</option>
              </select>
            </label>
            <label className="arc-field">
              <span>{t('arcade.cartridges.tier')}</span>
              <select value={tier} onChange={(e) => setTier(e.target.value as Tier)}>
                {TIERS.map((x) => (
                  <option key={x} value={x}>
                    {t(tierKey(x))}
                  </option>
                ))}
              </select>
            </label>
            <TextInput
              label={t('arcade.cartridges.source')}
              hint={t('arcade.cartridges.sourceHint')}
              value={source}
              onChange={(e) => setSource(e.target.value)}
            />
            {error && (
              <Notice
                variant="danger"
                title={t('arcade.cartridges.addFailed', { reason: error })}
              />
            )}
            <Button type="submit" disabled={!title.trim() || !source.trim()}>
              {t('arcade.cartridges.add')}
            </Button>
          </form>
        </>
      )}
    </section>
  );
}
