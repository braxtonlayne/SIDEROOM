import { Button, EmptyState, Icon, Modal, Notice, Spinner, TextInput } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useT } from '../i18n';
import { errorText } from '../settings/server/api';
import { useStore } from '../store/store';
import type { Guild } from '../store/types';
import type { ArcadeApi } from './api';
import {
  BUILTIN_GAMES,
  builtinArtFor,
  builtinsByStatus,
  heroSlots,
  liveCount,
  pickArt,
  type BuiltinGame,
} from './builtins';
import { GameArt } from './GameArt';
import type { Lobby } from '../activities/types';
import { GameCard } from './GameCard';
import type { PlayTarget } from './StartInDialog';
import { type Cartridge, type ComingSoonCard, type FeaturedCard, type Tier } from './types';
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

export const tierKey = (t: Tier) =>
  t === 'TIER_1_ORIGINAL' ? 'arcade.tier.original' : 'arcade.tier.reimplementation';

/** The tier as a card's corner tag (D-479): short enough for the painting's column; the full name is on the meta line. */
const tagKey = (t: Tier) =>
  t === 'TIER_1_ORIGINAL' ? 'arcade.tier.original' : 'arcade.tier.tagReimplementation';

/** A section label in the rooms' editorial style: a thin rule, the name in small caps, a muted note after it. */
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
      <div className="arc-label">
        <h2 id={id}>{title}</h2>
        {intro && <p className="arc-label-note">{intro}</p>}
      </div>
      {action}
    </div>
  );
}

/** Play on a card (D-472): opens the "Start in…" dialog for this game. */
export type PlayHandler = (target: PlayTarget) => void;

function PlayButton({ title, onClick }: { title: string; onClick: () => void }) {
  const t = useT();
  return (
    <Button size="sm" onClick={onClick} aria-label={t('arcade.playAria', { game: title })}>
      <Icon name="play" size={16} />
      {t('arcade.play')}
    </Button>
  );
}

/**
 * One built-in game. With `onPlay` it carries a Play button that starts it in a call or channel (D-472); without it
 * (the Showroom, the section tests) the card says where games start and is itself reachable by Tab.
 */
export function BuiltinCard({
  game,
  flags,
  onPlay,
  size = 'row',
  live = 0,
}: {
  game: BuiltinGame;
  flags: readonly CapabilityFlag[];
  onPlay?: PlayHandler;
  size?: 'row' | 'hero';
  /** Lobbies of this game in the strip (D-487): an "N live" chip on the meta line. */
  live?: number;
}) {
  const t = useT();
  const title = t(`arcade.builtin.${game.id}.name`);
  const players = game.players
    ? t('arcade.builtin.players', { min: game.players.min, max: game.players.max })
    : null;
  const playable = game.status === 'live' && onPlay;
  return (
    <GameCard
      size={size}
      title={title}
      description={t(`arcade.builtin.${game.id}.pitch`)}
      art={game.art}
      status={game.status}
      tag={game.tier ? t(tierKey(game.tier)) : undefined}
      meta={
        (players || (game.status === 'live' && !playable) || live > 0) && (
          <>
            <LiveChip count={live} />
            {players && (
              <span className="arc-meta-item">
                <Icon name="users" size={16} />
                {players}
              </span>
            )}
            {game.status === 'live' && !playable && (
              <span className="arc-meta-item">
                <Icon name="play" size={16} />
                {t('arcade.builtin.start')}
              </span>
            )}
          </>
        )
      }
      capabilities={game.capabilities}
      flags={flags}
      focusable={!playable}
    >
      {playable && (
        <div className="arc-actions">
          <PlayButton
            title={title}
            onClick={() =>
              onPlay({
                title,
                shelfName: game.shelfName,
                // D-478: board tools (no seats) list text channels first.
                kind: game.players ? 'seated' : 'board',
              })
            }
          />
        </div>
      )}
    </GameCard>
  );
}

/**
 * The top of the Arcade (the concept's hero): the room's headline on one side, two large cards on the other. The
 * cards are the Operator's browser-playable Featured picks at positions 1 and 2, then the shipped heroes (D-477), so
 * the built-in pair shows while the Front Page loads and on an instance with no picks.
 */
export function ArcadeHero({
  flags,
  onPlay,
  featured = [],
  shelfIds,
  games = BUILTIN_GAMES,
  lobbies = [],
}: {
  flags: readonly CapabilityFlag[];
  onPlay?: PlayHandler;
  featured?: readonly FeaturedCard[];
  shelfIds?: Readonly<Record<string, string>>;
  games?: readonly BuiltinGame[];
  /** The Active lobbies list, for the hero cards' "N live" chips (D-500). */
  lobbies?: readonly Lobby[];
}) {
  const t = useT();
  const slots = heroSlots(featured, games, shelfIds);
  return (
    <div className="arc-hero">
      <div className="arc-hero-text">
        <p className="arc-eyebrow">{t('arcade.hero.eyebrow')}</p>
        <h1 className="arc-hero-title">{t('arcade.hero.title')}</h1>
        <p className="arc-hero-sub">{t('arcade.hero.subline')}</p>
        <p className="arc-hero-how">{t(onPlay ? 'arcade.hero.how' : 'arcade.hero.howNoPlay')}</p>
      </div>
      {slots.length > 0 && (
        <div className="arc-hero-cards">
          {slots.map((slot) =>
            slot.kind === 'builtin' ? (
              <BuiltinCard
                key={slot.game.id}
                game={slot.game}
                flags={flags}
                onPlay={onPlay}
                size="hero"
                live={liveCount(lobbies, { name: slot.game.shelfName })}
              />
            ) : (
              <GameCard
                key={slot.pick.application_id}
                size="hero"
                title={slot.pick.name}
                description={slot.pick.description}
                blurb={slot.pick.blurb}
                art={pickArt(slot.pick, games, shelfIds)}
                status="live"
                tag={t(tagKey(slot.pick.tier))}
                meta={
                  liveCount(lobbies, { applicationId: slot.pick.application_id }) > 0 && (
                    <LiveChip
                      count={liveCount(lobbies, { applicationId: slot.pick.application_id })}
                    />
                  )
                }
              >
                {onPlay && (
                  <div className="arc-actions">
                    <PlayButton
                      title={slot.pick.name}
                      onClick={() =>
                        onPlay({
                          title: slot.pick.name,
                          applicationId: slot.pick.application_id,
                          kind: 'seated',
                        })
                      }
                    />
                  </div>
                )}
              </GameCard>
            ),
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Playable now (ARC-UI-010): the built-in games and the Emulator. They need no Front Page data, so the shelf draws
 * at once, before (and even without) the Operator's picks.
 */
export function PlayableSection({
  flags,
  emulatorOn,
  onPlay,
  games = BUILTIN_GAMES,
  lobbies = [],
}: {
  flags: readonly CapabilityFlag[];
  emulatorOn: boolean;
  onPlay?: PlayHandler;
  games?: readonly BuiltinGame[];
  /** The Active lobbies list, for the cards' "N live" chips (D-487). */
  lobbies?: readonly Lobby[];
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
        {live.map((g) => (
          <BuiltinCard
            key={g.id}
            game={g}
            flags={flags}
            onPlay={onPlay}
            live={liveCount(lobbies, { name: g.shelfName })}
          />
        ))}
        {emulatorOn && (
          <GameCard
            art="emulator"
            title={t('arcade.emulator.name')}
            description={t('arcade.emulator.description')}
            meta={<span className="arc-meta-item">{t('arcade.emulator.kicker')}</span>}
          >
            <div className="arc-actions">
              <Link to="/games/emulator" className="arc-cta" aria-label={t('arcade.emulator.open')}>
                {t('arcade.emulator.openShort')}
              </Link>
            </div>
          </GameCard>
        )}
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
  onPlay,
  games = BUILTIN_GAMES,
  shelfIds,
  lobbies = [],
}: {
  featured: FeaturedCard[];
  flags: CapabilityFlag[];
  onPlay?: PlayHandler;
  games?: readonly BuiltinGame[];
  /** The shelf's application ids by name, so a pick that is a built-in keeps the built-in's cover (D-477). */
  shelfIds?: Readonly<Record<string, string>>;
  /** The Active lobbies list, for the cards' "N live" chips (D-487). */
  lobbies?: readonly Lobby[];
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
            art={pickArt(f, games, shelfIds)}
            status={f.eligibility.browser_play ? 'live' : undefined}
            tag={t(tagKey(f.tier))}
            tier={f.tier}
            eligibility={f.eligibility}
            capabilities={f.capabilities}
            flags={flags}
            tips={f.tip_links}
            meta={
              liveCount(lobbies, { applicationId: f.application_id }) > 0 && (
                <LiveChip count={liveCount(lobbies, { applicationId: f.application_id })} />
              )
            }
            trust={{
              applicationId: f.application_id,
              name: f.name,
              codeSha256: f.code_sha256 ?? null,
            }}
          >
            {!f.eligibility.browser_play ? (
              // Fix 7: a desktop-only pick ends in one quiet sentence, never a dead end.
              <p className="arc-dim">{t('arcade.featured.desktopOnlyHint')}</p>
            ) : onPlay ? (
              <div className="arc-actions">
                <PlayButton
                  title={f.name}
                  onClick={() =>
                    onPlay({ title: f.name, applicationId: f.application_id, kind: 'seated' })
                  }
                />
              </div>
            ) : (
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
              tag={t(tagKey(e.tier))}
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

/** Who listed a cartridge (only when the app knows the name) and its file's SHA-256 (only when it has one). */
function cartridgeMeta(
  c: Cartridge,
  owner: string | null,
  t: (key: string, values?: Record<string, string>) => string,
): ReactNode {
  const sha = c.object_hash && /^[0-9a-f]{64}$/i.test(c.object_hash) ? c.object_hash : null;
  if (!owner && !sha) return null;
  return (
    <>
      {owner && <span className="arc-meta-item">{t('arcade.cartridges.by', { name: owner })}</span>}
      {sha && (
        <span className="arc-meta-item arc-sha">
          <span className="arc-sha-label">{t('arcade.cartridges.sha')}</span>
          {/* 12 hex characters on the storefront (Admin shows 8, D-407); the full hash is the title and is copied. */}
          <code title={sha}>
            <span aria-hidden="true">{sha.slice(0, 12)}…</span>
            <span className="arc-sr-only">{sha}</span>
          </code>
        </span>
      )}
    </>
  );
}

interface CartridgeDraft {
  title: string;
  description: string;
  kind: 'game' | 'mod';
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
  const [source, setSource] = useState('');

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void onSubmit({ title, description, kind, source }).then((ok) => {
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
        </div>
        <TextInput
          label={t('arcade.cartridges.source')}
          hint={t('arcade.cartridges.sourceHint')}
          value={source}
          onChange={(e) => setSource(e.target.value)}
        />
        <p className="arc-form-note">{t('arcade.cartridges.tierFromManifest')}</p>
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
  openSignal = 0,
}: {
  guilds: Pick<Guild, 'id' | 'name'>[];
  flags: CapabilityFlag[];
  /** Bumped by the page's "+ List a cartridge" in the pill row: opens the same dialog. */
  openSignal?: number;
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
        source_ref: i.source,
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
  useEffect(() => {
    if (openSignal > 0) {
      setAddError(null);
      setDialog(true);
    }
  }, [openSignal]);
  const users = useStore((st) => st.users);
  const ownerName = (id: string) => {
    const u = users[id];
    return u ? (u.global_name ?? u.username) : null;
  };

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
            <div className="arc-list">
              {items.map((c) => (
                <GameCard
                  key={c.id}
                  size="list"
                  title={c.title}
                  description={c.description}
                  art="cartridge"
                  meta={cartridgeMeta(c, ownerName(c.added_by), t)}
                  badge={t(
                    c.kind === 'mod' ? 'arcade.cartridges.kindMod' : 'arcade.cartridges.kindGame',
                  )}
                  tag={t(tagKey(c.tier))}
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
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('arcade.cartridges.remove', { title: c.title })}
                      onClick={() => remove(c.id)}
                    >
                      {t('arcade.cartridges.removeShort')}
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

/** A card's "N live" chip (D-487): its game has lobbies in the strip above; it links there. */
export function LiveChip({ count }: { count: number }) {
  const t = useT();
  if (count <= 0) return null;
  return (
    <a
      href="#lobbies"
      className="arc-meta-item arc-live"
      aria-label={t('arcade.lobbies.liveAria', { n: count })}
    >
      <span className="arc-live-dot" aria-hidden="true" />
      {t('arcade.lobbies.live', { n: count })}
    </a>
  );
}

/** Minutes and hours since a lobby started, as the card's count line says it. */
function sinceText(t: ReturnType<typeof useT>, createdAtMs: number, now: number): string {
  const m = Math.max(0, Math.floor((now - createdAtMs) / 60_000));
  return m >= 60
    ? t('arcade.lobbies.sinceHours', { h: Math.floor(m / 60) })
    : t('arcade.lobbies.since', { m });
}

/** How often a lobby card's "for N min" is worked out again in the browser; nothing is fetched (D-501). */
export const LOBBY_CLOCK_MS = 60_000;

/** What a Join on a lobby card ended in, shown under its button (D-486). */
export type LobbyError = 'call' | 'ended' | 'failed';

const LOBBY_ERROR_KEY: Record<LobbyError, string> = {
  call: 'arcade.start.joinFailed',
  ended: 'arcade.lobbies.ended',
  failed: 'arcade.lobbies.failed',
};

/**
 * Active lobbies (ARC-LOB-001, D-486): the games running in the member's servers that they could join now, one row of
 * compact cards that scrolls sideways. Hidden entirely while there are none (no empty state, D-474).
 */
export function LobbiesSection({
  items,
  onJoin,
  busy = null,
  errors = {},
  now: initialNow = Date.now(),
}: {
  items: readonly Lobby[];
  onJoin: (lobby: Lobby) => void;
  /** The instance whose Join is under way: its button is busy and the others wait. */
  busy?: string | null;
  /** A card's error line, by instance id. */
  errors?: Readonly<Record<string, LobbyError>>;
  /** The clock's first reading; it then ticks every [LOBBY_CLOCK_MS] (D-501). */
  now?: number;
}) {
  const t = useT();
  const [now, setNow] = useState(initialNow);
  // A new list is read against the time it arrived, not the time the page mounted.
  const [seen, setSeen] = useState(items);
  if (seen !== items) {
    setSeen(items);
    setNow(Date.now());
  }
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), LOBBY_CLOCK_MS);
    return () => clearInterval(id);
  }, []);
  if (items.length === 0) return null;
  return (
    <section aria-labelledby="arc-lobbies" id="lobbies" className="arc-section">
      <SectionHead
        id="arc-lobbies"
        title={t('arcade.lobbies.heading')}
        intro={t('arcade.lobbies.note')}
      />
      <ul className="arc-lobbies" aria-label={t('arcade.lobbies.list')}>
        {items.map((l) => {
          const voice = l.channel_type === 'voice';
          const where = { server: l.guild_name, channel: l.channel_name };
          const error = errors[l.instance_id];
          return (
            <li key={l.instance_id} className="arc-lobby">
              <div className="arc-lobby-art">
                <GameArt kind={builtinArtFor(l.application_name)} seed={l.application_name} />
              </div>
              <div className="arc-lobby-body">
                <h3 className="arc-lobby-title">{l.application_name}</h3>
                {l.development && (
                  <p className="arc-blurb" data-tone="preview">
                    {t('arcade.lobbies.development')}
                  </p>
                )}
                <p
                  className="arc-meta arc-lobby-where"
                  aria-label={
                    voice
                      ? t('arcade.lobbies.whereVoiceAria', where)
                      : t('arcade.lobbies.whereText', where)
                  }
                >
                  <Icon name={voice ? 'volume' : 'hash'} size={16} />
                  <span className="arc-lobby-where-text" aria-hidden="true">
                    {t(voice ? 'arcade.lobbies.whereVoice' : 'arcade.lobbies.whereText', where)}
                  </span>
                </p>
                <p className="arc-meta">
                  <span className="arc-meta-item">
                    <Icon name="users" size={16} />
                    {t('arcade.lobbies.players', { n: l.player_count })}
                  </span>
                  <span className="arc-meta-item">{sinceText(t, l.created_at_ms, now)}</span>
                </p>
                <Button
                  size="sm"
                  fullWidth
                  variant={l.joined ? 'secondary' : 'primary'}
                  className="arc-lobby-join"
                  disabled={busy !== null && busy !== l.instance_id}
                  loading={busy === l.instance_id}
                  leadingIcon={<Icon name={l.joined ? 'door' : 'play'} size={16} />}
                  aria-label={t(
                    l.joined ? 'arcade.lobbies.returnAria' : 'arcade.lobbies.joinAria',
                    {
                      game: l.application_name,
                      channel: l.channel_name,
                    },
                  )}
                  onClick={() => onJoin(l)}
                >
                  {t(l.joined ? 'arcade.lobbies.return' : 'arcade.lobbies.join')}
                </Button>
                {error && (
                  <p className="arc-start-error arc-lobby-error" role="alert">
                    {t(LOBBY_ERROR_KEY[error])}
                  </p>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
