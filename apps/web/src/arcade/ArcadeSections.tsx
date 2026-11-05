import { Button, EmptyState, Icon, Modal, Notice, Spinner, TextInput } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useT } from '../i18n';
import { errorText } from '../settings/server/api';
import type { Guild } from '../store/types';
import type { ArcadeApi } from './api';
import { BUILTIN_GAMES, builtinsByStatus, type BuiltinGame } from './builtins';
import { GameCard } from './GameCard';
import { TIERS, type Cartridge, type ComingSoonCard, type FeaturedCard, type Tier } from './types';
import type { CapabilityFlag } from './types';
import { apiFetch } from '../api/client';
import { objectPath } from '../study/objectUrl';

/**
 * Open a cartridge's shelf object (wave 41): the per-object URL route checks the member's own grant and every
 * delisting when it mints a ten-minute URL for that one file.
 */
async function openShelfObject(shelfId: string, hash: string): Promise<void> {
  const r = await apiFetch<{ url: string }>(`${objectPath(shelfId, hash)}/url`, { retries: 0 });
  window.open(r.url, '_blank', 'noopener,noreferrer');
}

const tierKey = (t: Tier) =>
  t === 'TIER_1_ORIGINAL' ? 'arcade.tier.original' : 'arcade.tier.reimplementation';

/** A section heading with its one-line intro and, optionally, a control at the end of the row. */
export function SectionHead({
  id,
  title,
  intro,
  action,
}: {
  id: string;
  title: string;
  intro?: string;
  action?: ReactNode;
}) {
  return (
    <div className="arc-section-head">
      <div className="arc-section-title">
        <h2 id={id}>{title}</h2>
        {intro && <p className="arc-dim">{intro}</p>}
      </div>
      {action}
    </div>
  );
}

/** One built-in game (ARC-UI-010): started from a channel, so the card carries no button of its own. */
export function BuiltinCard({
  game,
  flags,
}: {
  game: BuiltinGame;
  flags: readonly CapabilityFlag[];
}) {
  const t = useT();
  const players = game.players
    ? t('arcade.builtin.players', { min: game.players.min, max: game.players.max })
    : null;
  return (
    <GameCard
      title={t(`arcade.builtin.${game.id}.name`)}
      description={t(`arcade.builtin.${game.id}.pitch`)}
      art={game.art}
      status={game.status}
      meta={
        <>
          {players && (
            <span className="arc-meta-item">
              <Icon name="users" size={16} />
              {players}
            </span>
          )}
          {game.status === 'live' && (
            <span className="arc-meta-item">
              <Icon name="play" size={16} />
              {t('arcade.builtin.start')}
            </span>
          )}
        </>
      }
      capabilities={game.capabilities}
      flags={flags}
      focusable
    />
  );
}

/**
 * Playable now (ARC-UI-010): the built-in games and the Emulator. They need no Front Page data, so the shelf draws
 * at once, before (and even without) the Operator's picks.
 */
export function PlayableSection({
  flags,
  emulatorOn,
  games = BUILTIN_GAMES,
}: {
  flags: readonly CapabilityFlag[];
  emulatorOn: boolean;
  games?: readonly BuiltinGame[];
}) {
  const t = useT();
  const { live } = builtinsByStatus(games);
  return (
    <section aria-labelledby="arc-playable" id="playable" className="arc-section">
      <SectionHead
        id="arc-playable"
        title={t('arcade.playable.heading')}
        intro={t('arcade.playable.intro')}
      />
      <div className="arc-grid">
        {emulatorOn && (
          <GameCard
            size="feature"
            art="emulator"
            blurb={t('arcade.emulator.kicker')}
            title={t('arcade.emulator.name')}
            description={t('arcade.emulator.description')}
          >
            <Link to="/games/emulator" className="arc-cta">
              {t('arcade.emulator.open')}
              <span aria-hidden="true"> →</span>
            </Link>
          </GameCard>
        )}
        {live.map((g) => (
          <BuiltinCard key={g.id} game={g} flags={flags} />
        ))}
      </div>
    </section>
  );
}

/**
 * Featured / Front Page: the Operator's picks of community games. Hidden while there are none (D-474): the Operator
 * finds the picks in Admin > Arcade, and an empty shelf reads as broken.
 */
export function FeaturedSection({
  featured,
  flags,
}: {
  featured: FeaturedCard[];
  flags: CapabilityFlag[];
}) {
  const t = useT();
  if (featured.length === 0) return null;
  return (
    <section aria-labelledby="arc-featured" id="featured" className="arc-section">
      <SectionHead
        id="arc-featured"
        title={t('arcade.featured.heading')}
        intro={t('arcade.featured.intro')}
      />
      <div className="arc-grid">
        {featured.map((f) => (
          <GameCard
            key={f.application_id}
            title={f.name}
            description={f.description}
            blurb={f.blurb}
            art="featured"
            status={f.eligibility.browser_play ? 'live' : undefined}
            tier={f.tier}
            eligibility={f.eligibility}
            capabilities={f.capabilities}
            flags={flags}
            tips={f.tip_links}
            trust={{
              applicationId: f.application_id,
              name: f.name,
              codeSha256: f.code_sha256 ?? null,
            }}
          >
            {f.eligibility.browser_play && (
              <p className="arc-dim">{t('arcade.featured.startHintAny')}</p>
            )}
          </GameCard>
        ))}
      </div>
    </section>
  );
}

/** Coming Soon showcase with the per-member Notify me toggle, and any built-in game still in development. */
export function ComingSoonSection({
  entries,
  api,
  flags = [],
  games = BUILTIN_GAMES,
}: {
  entries: ComingSoonCard[];
  api: Pick<ArcadeApi, 'setNotify'>;
  flags?: readonly CapabilityFlag[];
  games?: readonly BuiltinGame[];
}) {
  const t = useT();
  const { preview } = builtinsByStatus(games);
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
  const empty = entries.length === 0 && preview.length === 0;
  return (
    <section aria-labelledby="arc-soon" id="coming-soon" className="arc-section">
      <SectionHead id="arc-soon" title={t('arcade.soon.heading')} intro={t('arcade.soon.intro')} />
      {failed && <Notice variant="danger" title={t('arcade.soon.notifyFailed')} />}
      {empty ? (
        <EmptyState
          className="arc-empty"
          icon={<Icon name="sparkle" size={32} />}
          title={t('arcade.soon.emptyTitle')}
          description={t('arcade.soon.emptyBody')}
        />
      ) : (
        <div className="arc-grid">
          {preview.map((g) => (
            <BuiltinCard key={g.id} game={g} flags={flags} />
          ))}
          {entries.map((e) => (
            <GameCard
              key={e.id}
              title={e.title}
              description={e.summary}
              art="workshop"
              status="preview"
              tier={e.tier}
              eligibility={{
                browser_play: e.tier === 'TIER_1_ORIGINAL',
                guest_links: e.tier === 'TIER_1_ORIGINAL',
                featured: true,
                desktop_only: e.tier === 'TIER_2_REIMPLEMENTATION',
                needs_base_game_hint: e.tier === 'TIER_2_REIMPLEMENTATION',
              }}
            >
              <div className="arc-actions">
                <Button
                  size="sm"
                  variant={notify[e.id] ? 'primary' : 'secondary'}
                  aria-pressed={!!notify[e.id]}
                  onClick={() => toggle(e.id)}
                >
                  {t('arcade.soon.notify')}
                </Button>
                <a href={e.repo_url} target="_blank" rel="noopener noreferrer" className="arc-link">
                  {t('arcade.soon.repo')}
                </a>
              </div>
              {notify[e.id] && (
                <p className="arc-dim arc-notifying">
                  <Icon name="check" size={16} />
                  {t('arcade.soon.notifying')}
                </p>
              )}
            </GameCard>
          ))}
        </div>
      )}
    </section>
  );
}

interface CartridgeDraft {
  title: string;
  description: string;
  kind: 'game' | 'mod';
  tier: Tier;
  source: string;
}

/** The "List a cartridge" form, in a dialog (it used to sit open on the page). */
function ListCartridgeDialog({
  open,
  onClose,
  serverName,
  onSubmit,
  error,
  busy,
}: {
  open: boolean;
  onClose: () => void;
  serverName: string;
  onSubmit: (input: CartridgeDraft) => Promise<boolean>;
  error: string | null;
  busy: boolean;
}) {
  const t = useT();
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [kind, setKind] = useState<'game' | 'mod'>('game');
  const [tier, setTier] = useState<Tier>('TIER_1_ORIGINAL');
  const [source, setSource] = useState('');

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void onSubmit({ title, description, kind, tier, source }).then((ok) => {
      if (!ok) return;
      setTitle('');
      setDescription('');
      setSource('');
    });
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={
        serverName
          ? t('arcade.cartridges.listIn', { server: serverName })
          : t('arcade.cartridges.addHeading')
      }
      description={t('arcade.cartridges.ipRule')}
    >
      <form className="arc-form" onSubmit={submit}>
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
        <div className="arc-form-row">
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
        </div>
        <TextInput
          label={t('arcade.cartridges.source')}
          hint={t('arcade.cartridges.sourceHint')}
          value={source}
          onChange={(e) => setSource(e.target.value)}
        />
        {error && (
          <Notice variant="danger" title={t('arcade.cartridges.addFailed', { reason: error })} />
        )}
        <div className="arc-form-actions">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t('arcade.cartridges.cancel')}
          </Button>
          <Button type="submit" loading={busy} disabled={!title.trim() || !source.trim()}>
            {t('arcade.cartridges.add')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

/** Guild Cartridges (unlisted): a server picker, its cartridges and the dialog that lists one. */
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
  const [picked, setGuildId] = useState(guilds[0]?.id ?? '');
  // The list can arrive after the first render (fetched before the gateway is up): fall back to its first server.
  const guildId = guilds.some((g) => g.id === picked) ? picked : (guilds[0]?.id ?? '');
  const serverName = guilds.find((g) => g.id === guildId)?.name ?? '';
  const [items, setItems] = useState<Cartridge[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [addError, setAddError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dialog, setDialog] = useState(false);
  const [loadFailed, setLoadFailed] = useState(false);
  const [openFailed, setOpenFailed] = useState(false);

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

  const add = async (i: CartridgeDraft): Promise<boolean> => {
    setAddError(null);
    setBusy(true);
    try {
      const c = await api.addCartridge(guildId, {
        title: i.title,
        description: i.description,
        kind: i.kind,
        tier: i.tier,
        source_ref: i.source,
        capabilities: [],
        tip_links: [],
      });
      setItems((cur) => [c, ...(cur ?? [])]);
      setDialog(false);
      return true;
    } catch (err: unknown) {
      setAddError(errorText(err, ''));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const remove = (id: string) => {
    setError(null);
    api
      .removeCartridge(guildId, id)
      .then(() => setItems((cur) => (cur ?? []).filter((c) => c.id !== id)))
      .catch((err: unknown) => setError(errorText(err, '')));
  };
  const listButton = (
    <Button
      variant="secondary"
      onClick={() => {
        setAddError(null);
        setDialog(true);
      }}
    >
      <Icon name="plus" size={16} />
      {t('arcade.cartridges.listButton')}
    </Button>
  );
  const empty = items?.length === 0;

  return (
    <section aria-labelledby="arc-carts" id="cartridges" className="arc-section">
      <SectionHead
        id="arc-carts"
        title={t('arcade.cartridges.heading')}
        intro={t('arcade.cartridges.intro')}
        action={
          guilds.length > 0 && (loadFailed || (items?.length ?? 0) > 0) ? listButton : undefined
        }
      />
      {guilds.length === 0 ? (
        <EmptyState
          className="arc-empty"
          icon={<Icon name="users" size={32} />}
          title={t('arcade.cartridges.noneTitle')}
          description={t('arcade.cartridges.none')}
        />
      ) : (
        <>
          {guilds.length > 1 && (
            <label className="arc-field arc-server-pick">
              <span>{t('arcade.cartridges.server')}</span>
              <select value={guildId} onChange={(e) => setGuildId(e.target.value)}>
                {guilds.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </select>
            </label>
          )}
          {loadFailed && <Notice variant="danger" title={t('arcade.cartridges.failed')} />}
          {openFailed && <Notice variant="danger" title={t('arcade.cartridges.openFailed')} />}
          {error && (
            <Notice variant="danger" title={t('arcade.cartridges.addFailed', { reason: error })} />
          )}
          {items === null && !loadFailed && (
            <div className="arc-loading">
              <Spinner label={t('arcade.cartridges.loading')} />
            </div>
          )}
          {items !== null && items.length > 0 && (
            <div className="arc-grid">
              {items.map((c) => (
                <GameCard
                  key={c.id}
                  title={c.title}
                  description={c.description}
                  art="cartridge"
                  badge={t(
                    c.kind === 'mod' ? 'arcade.cartridges.kindMod' : 'arcade.cartridges.kindGame',
                  )}
                  tier={c.tier}
                  eligibility={c.eligibility}
                  capabilities={c.capabilities}
                  flags={flags}
                  tips={c.tip_links}
                >
                  <div className="arc-actions">
                    {c.shelf_id && c.object_hash && (
                      <Button
                        size="sm"
                        onClick={() => {
                          setOpenFailed(false);
                          openShelfObject(c.shelf_id ?? '', c.object_hash ?? '').catch(() =>
                            setOpenFailed(true),
                          );
                        }}
                      >
                        {t('arcade.cartridges.open', { title: c.title })}
                      </Button>
                    )}
                    <Button size="sm" variant="ghost" onClick={() => remove(c.id)}>
                      {t('arcade.cartridges.remove', { title: c.title })}
                    </Button>
                  </div>
                </GameCard>
              ))}
            </div>
          )}
          {empty && (
            <EmptyState
              className="arc-empty"
              icon={<Icon name="grid" size={32} />}
              title={t('arcade.cartridges.emptyTitle', { server: serverName })}
              description={t('arcade.cartridges.emptyBody')}
              action={listButton}
            />
          )}
          <ListCartridgeDialog
            open={dialog}
            onClose={() => setDialog(false)}
            serverName={serverName}
            onSubmit={add}
            error={addError}
            busy={busy}
          />
        </>
      )}
    </section>
  );
}
