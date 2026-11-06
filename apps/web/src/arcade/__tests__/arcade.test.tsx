// ARC-ROOM-001: the Arcade sections (Featured, Guild Cartridges, Coming Soon), tip badges and the trust seam.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useEffect, type ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { ArcadeApi } from '../api';
import {
  ArcadeHero,
  CartridgesSection,
  ComingSoonSection,
  FeaturedSection,
  PlayableSection,
} from '../ArcadeSections';
import { builtinsByStatus, heroSlots, type BuiltinGame } from '../builtins';
import { memberFlags } from '../GameCard';
import { sandboxTrust } from '../sandboxTrust';
import { http, HttpResponse, server } from '../../test/server';
import { artKind, GameArt, hueOf } from '../GameArt';
import { TipBadges } from '../TipBadges';
import { TrustProvider } from '../trust';
import type {
  CapabilityFlag,
  Cartridge,
  ComingSoonCard,
  Eligibility,
  FeaturedCard,
} from '../types';

// The Emulator card links with the router's Link; these section tests render without a router.
vi.mock('@tanstack/react-router', () => ({
  Link: ({ to, children, ...rest }: { to: string; children: ReactNode; className?: string }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}));

const T1: Eligibility = {
  browser_play: true,
  guest_links: true,
  featured: true,
  desktop_only: false,
  needs_base_game_hint: false,
};
const T2: Eligibility = {
  browser_play: false,
  guest_links: false,
  featured: true,
  desktop_only: true,
  needs_base_game_hint: true,
};
const flags: CapabilityFlag[] = [
  { name: 'datachannel', status: 'active', summary: 'data' },
  { name: 'guest_play', status: 'reserved', summary: 'guests' },
];
const featured = (over: Partial<FeaturedCard> = {}): FeaturedCard => ({
  application_id: '10',
  name: 'Land.io',
  description: 'A roguelike',
  blurb: 'Our flagship',
  tier: 'TIER_1_ORIGINAL',
  eligibility: T1,
  capabilities: ['datachannel', 'guest_play'],
  tip_links: [{ kind: 'kofi', url: 'https://ko-fi.com/dev' }],
  bundle_sha256: 'a'.repeat(64),
  code_sha256: 'c'.repeat(64),
  position: 0,
  featured_at_ms: 1,
  ...over,
});

describe('Featured', () => {
  it('shows the Operator picks with tier, capability flags and tips', () => {
    render(
      <FeaturedSection
        featured={[
          featured(),
          featured({
            application_id: '11',
            name: 'Horizon',
            tier: 'TIER_2_REIMPLEMENTATION',
            eligibility: T2,
            capabilities: [],
            tip_links: [],
          }),
        ]}
        flags={flags}
      />,
    );
    expect(screen.getByRole('heading', { name: 'Featured' })).toBeInTheDocument();
    const land = screen.getByRole('heading', { name: 'Land.io' }).closest('article')!;
    expect(within(land).getByText('Original')).toBeInTheDocument();
    // D-479: plumbing (datachannel) and a reserved flag (guest_play here) never show on a member card.
    expect(within(land).queryByText('Shared state')).toBeNull();
    expect(within(land).queryByText('Guest play')).toBeNull();
    expect(within(land).queryByText(/planned/)).toBeNull();
    expect(within(land).getByRole('link', { name: 'Ko-fi' })).toHaveAttribute(
      'href',
      'https://ko-fi.com/dev',
    );
    const horizon = screen.getByRole('heading', { name: 'Horizon' }).closest('article')!;
    expect(within(horizon).getByText('Desktop app only')).toBeInTheDocument();
    expect(within(horizon).queryByText(/Start it from a channel/)).toBeNull();
  });

  it('is not on the page while nothing is featured (D-474)', () => {
    const { container } = render(<FeaturedSection featured={[]} flags={flags} />);
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole('heading', { name: 'Featured' })).toBeNull();
  });

  it('appears with its intro and provenance line once there is a pick (D-475)', () => {
    render(<FeaturedSection featured={[featured()]} flags={flags} />);
    expect(screen.getByRole('region', { name: 'Featured' })).toBeInTheDocument();
    expect(
      screen.getByText(
        'Community games the Operator picked. Each one says who owns its code, art and sound.',
      ),
    ).toBeInTheDocument();
  });

  it('shows the badges and actions the trust provider supplies for a build hash', () => {
    const badges = vi.fn(() => [
      { id: 'sig', label: 'SWARM-SAFETY-SIGNED', tone: 'good' as const },
    ]);
    render(
      <TrustProvider source={{ badges, actions: (c) => <button>Vouch {c.name}</button> }}>
        <FeaturedSection featured={[featured()]} flags={flags} />
      </TrustProvider>,
    );
    expect(screen.getByText('SWARM-SAFETY-SIGNED')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Vouch Land.io' })).toBeInTheDocument();
    // Keyed on the code hash (what signatures and vouches are pinned to), not the bundle hash.
    expect(badges).toHaveBeenCalledWith({
      applicationId: '10',
      name: 'Land.io',
      codeSha256: 'c'.repeat(64),
    });
  });

  it('renders a trust Card per build and remounts it when the code hash changes', () => {
    const mounts: string[] = [];
    function Card({ card }: { card: { codeSha256: string | null } }) {
      useEffect(() => {
        mounts.push(card.codeSha256 ?? '');
      }, [card.codeSha256]);
      return <span>trust of {card.codeSha256?.slice(0, 8)}</span>;
    }
    const source = { badges: () => [], Card };
    const { rerender } = render(
      <TrustProvider source={source}>
        <FeaturedSection featured={[featured()]} flags={flags} />
      </TrustProvider>,
    );
    expect(screen.getByText('trust of cccccccc')).toBeInTheDocument();
    rerender(
      <TrustProvider source={source}>
        <FeaturedSection featured={[featured({ code_sha256: 'd'.repeat(64) })]} flags={flags} />
      </TrustProvider>,
    );
    expect(screen.getByText('trust of dddddddd')).toBeInTheDocument();
    expect(mounts).toEqual(['c'.repeat(64), 'd'.repeat(64)]);
  });
});

describe('Tip badges', () => {
  it('draw only links that pass the host allow-list', () => {
    render(
      <TipBadges
        name="Dev"
        links={[
          { kind: 'kofi', url: 'https://evil.example/dev' },
          { kind: 'patreon', url: 'https://www.patreon.com/dev' },
        ]}
      />,
    );
    expect(screen.queryByRole('link', { name: 'Ko-fi' })).toBeNull();
    const a = screen.getByRole('link', { name: 'Patreon' });
    expect(a).toHaveAttribute('rel', expect.stringContaining('noopener'));
  });

  it('show a QR code for a Bitcoin link on demand', async () => {
    const user = userEvent.setup();
    render(
      <TipBadges
        name="Dev"
        links={[{ kind: 'btc', url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq' }]}
      />,
    );
    expect(screen.queryByRole('img')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Bitcoin' }));
    expect(screen.getByRole('img', { name: 'Bitcoin QR code to tip Dev' })).toBeInTheDocument();
  });

  it('draw nothing without links', () => {
    const { container } = render(<TipBadges name="Dev" links={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('Coming Soon', () => {
  const entry: ComingSoonCard = {
    id: '5',
    title: 'OpenMW',
    summary: 'A clean-room engine',
    tier: 'TIER_2_REIMPLEMENTATION',
    repo_url: 'https://github.com/OpenMW/openmw',
    notify: false,
    created_at_ms: 1,
  };

  it('links the repository and toggles Notify me', async () => {
    const user = userEvent.setup();
    const setNotify = vi.fn(() => Promise.resolve());
    render(<ComingSoonSection entries={[entry]} api={{ setNotify }} />);
    expect(screen.getByRole('link', { name: 'Repository' })).toHaveAttribute(
      'href',
      'https://github.com/OpenMW/openmw',
    );
    const btn = screen.getByRole('button', { name: 'Notify me' });
    expect(btn).toHaveAttribute('aria-pressed', 'false');
    await user.click(btn);
    expect(setNotify).toHaveBeenCalledWith('5', true);
    expect(btn).toHaveAttribute('aria-pressed', 'true');
    await user.click(btn);
    expect(setNotify).toHaveBeenLastCalledWith('5', false);
  });

  it('undoes the toggle when saving fails', async () => {
    const user = userEvent.setup();
    const setNotify = vi.fn(() => Promise.reject(new Error('x')));
    render(<ComingSoonSection entries={[entry]} api={{ setNotify }} />);
    const btn = screen.getByRole('button', { name: 'Notify me' });
    await user.click(btn);
    await waitFor(() => expect(btn).toHaveAttribute('aria-pressed', 'false'));
    expect(screen.getByText('That choice could not be saved.')).toBeInTheDocument();
  });
});

describe('Guild cartridges', () => {
  const cart: Cartridge = {
    id: '7',
    guild_id: '1',
    title: 'Pong',
    description: 'Two paddles',
    kind: 'game',
    tier: 'TIER_1_ORIGINAL',
    eligibility: T1,
    source_ref: 'games/pong.zip',
    capabilities: [],
    tip_links: [],
    added_by: '2',
    added_at_ms: 1,
  };
  const api = (over: Partial<ArcadeApi> = {}) => ({
    cartridges: vi.fn(() => Promise.resolve([cart])),
    addCartridge: vi.fn((_g: string, i: { title: string }) =>
      Promise.resolve({ ...cart, id: '8', title: i.title }),
    ),
    removeCartridge: vi.fn(() => Promise.resolve()),
    ...over,
  });

  it('lists the picked server cartridges and offers only the two tiers', async () => {
    const user = userEvent.setup();
    render(<CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={api()} />);
    expect(await screen.findByRole('heading', { name: 'Pong' })).toBeInTheDocument();
    // ARC-UI-010: the form lives in a dialog behind a button, not open on the page.
    expect(screen.queryByLabelText('Content tier')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'List a cartridge' }));
    const dialog = screen.getByRole('dialog', { name: 'List a cartridge in Friends' });
    const tier = within(dialog).getByLabelText('Content tier') as HTMLSelectElement;
    expect([...tier.options].map((o) => o.value)).toEqual([
      'TIER_1_ORIGINAL',
      'TIER_2_REIMPLEMENTATION',
    ]);
  });

  it('offers the shelf file only for a cartridge on a shelf object (wave 41)', async () => {
    const onShelf = {
      ...cart,
      id: '9',
      title: 'Snake',
      shelf_id: '5',
      object_hash: 'c'.repeat(64),
    };
    const a = api({ cartridges: vi.fn(() => Promise.resolve([cart, onShelf])) });
    render(<CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={a} />);
    expect(
      await screen.findByRole('button', { name: 'Get Snake from the shelf' }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Get Pong from the shelf' })).toBeNull();
  });

  it('lists and unlists a cartridge', async () => {
    const user = userEvent.setup();
    const a = api();
    render(<CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={a} />);
    await screen.findByRole('heading', { name: 'Pong' });
    await user.click(screen.getByRole('button', { name: 'List a cartridge' }));
    await user.type(screen.getByLabelText('Title'), 'Tetris-like');
    await user.type(screen.getByLabelText('Shelf path'), 'games/t.zip');
    await user.click(screen.getByRole('button', { name: 'List cartridge' }));
    expect(a.addCartridge).toHaveBeenCalledWith(
      '1',
      expect.objectContaining({
        title: 'Tetris-like',
        tier: 'TIER_1_ORIGINAL',
        source_ref: 'games/t.zip',
      }),
    );
    expect(await screen.findByRole('heading', { name: 'Tetris-like' })).toBeInTheDocument();
    // The dialog closes once the cartridge is listed.
    expect(screen.queryByRole('dialog')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Unlist Pong' }));
    expect(a.removeCartridge).toHaveBeenCalledWith('1', '7');
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Pong' })).toBeNull());
  });

  it('says so when the person is in no server', () => {
    render(<CartridgesSection guilds={[]} flags={flags} api={api()} />);
    expect(screen.getByText('You are not in a server yet.')).toBeInTheDocument();
  });
});

describe('ARC-UI-010: the redesigned shelves', () => {
  const cart: Cartridge = {
    id: '7',
    guild_id: '1',
    title: 'Pong',
    description: 'Two paddles',
    kind: 'game',
    tier: 'TIER_1_ORIGINAL',
    eligibility: T1,
    source_ref: 'games/pong.zip',
    capabilities: [],
    tip_links: [],
    added_by: '2',
    added_at_ms: 1,
  };
  const cartApi = (list: Cartridge[] = []) => ({
    cartridges: vi.fn((guildId: string) => Promise.resolve(guildId ? list : [])),
    addCartridge: vi.fn(() => Promise.reject(new Error('nope'))),
    removeCartridge: vi.fn(() => Promise.resolve()),
  });

  it('Playable now shows every built-in game with its art and readable capabilities, and the Emulator', () => {
    // The live catalogue: guest_play is active (the fixture above marks it reserved).
    render(<PlayableSection flags={[]} emulatorOn />);
    const region = screen.getByRole('region', { name: 'Playable now' });
    for (const name of ['Land.io', 'Arena 3D', 'Hold’em', 'Whiteboard', 'Tabletop']) {
      const card = within(region).getByRole('article', { name });
      expect(card.querySelector('svg[data-art]')).not.toBeNull();
      // Reachable by Tab although it has no button of its own.
      expect(card).toHaveAttribute('tabindex', '0');
      // The section says "Playable now"; the card carries it as its status.
      expect(card).toHaveAttribute('data-status', 'live');
    }
    const land = within(region).getByRole('article', { name: 'Land.io' });
    expect(within(land).getByText('1 to 24 players')).toBeInTheDocument();
    expect(within(land).getByText('Guest play')).toBeInTheDocument();
    const emu = within(region).getByRole('article', { name: 'Emulator' });
    expect(within(emu).getByRole('link', { name: /Open the emulator/ })).toHaveAttribute(
      'href',
      '/games/emulator',
    );
  });

  it('hides the Emulator card when the emulator is switched off', () => {
    render(<PlayableSection flags={flags} emulatorOn={false} />);
    expect(screen.queryByRole('article', { name: 'Emulator' })).toBeNull();
    expect(screen.getByRole('article', { name: 'Land.io' })).toBeInTheDocument();
  });

  it('a built-in still in development sits under Coming soon, not Playable now', () => {
    const games: BuiltinGame[] = [
      {
        id: 'landio',
        packageId: 'p1',
        shelfName: 'Land.io',
        status: 'live',
        capabilities: [],
        art: 'landio',
      },
      {
        id: 'whiteboard',
        packageId: 'p2',
        shelfName: 'Whiteboard',
        status: 'preview',
        capabilities: [],
        art: 'nope',
      },
    ];
    expect(builtinsByStatus(games).live.map((g) => g.id)).toEqual(['landio']);
    render(
      <>
        <PlayableSection flags={flags} emulatorOn={false} games={games} />
        <ComingSoonSection entries={[]} api={{ setNotify: vi.fn() }} games={games} />
      </>,
    );
    const soon = screen.getByRole('region', { name: 'Coming soon' });
    const wb = within(soon).getByRole('article', { name: 'Whiteboard' });
    expect(within(wb).getByText('In development')).toBeInTheDocument();
    // An art kind without a painting falls back to the generic one.
    expect(wb.querySelector('svg[data-art="featured"]')).not.toBeNull();
    const live = screen.getByRole('region', { name: 'Playable now' });
    expect(within(live).queryByRole('article', { name: 'Whiteboard' })).toBeNull();
  });

  it('Coming soon explains itself when the showcase is empty', () => {
    render(<ComingSoonSection entries={[]} api={{ setNotify: vi.fn() }} games={[]} />);
    expect(screen.getByText('Nothing in the workshop right now')).toBeInTheDocument();
  });

  it('an empty server explains cartridges and offers the dialog from the empty state', async () => {
    const user = userEvent.setup();
    render(
      <CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={cartApi()} />,
    );
    expect(await screen.findByText('No cartridges in Friends yet')).toBeInTheDocument();
    expect(screen.getByText(/A cartridge is a game or mod a server keeps/)).toBeInTheDocument();
    // One server: no picker.
    expect(screen.queryByLabelText('Server')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'List a cartridge' }));
    expect(screen.getByRole('dialog', { name: 'List a cartridge in Friends' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('offers the server picker only with more than one server', async () => {
    const user = userEvent.setup();
    const a = cartApi([cart]);
    render(
      <CartridgesSection
        guilds={[
          { id: '1', name: 'Friends' },
          { id: '2', name: 'Work' },
        ]}
        flags={flags}
        api={a}
      />,
    );
    await screen.findByRole('heading', { name: 'Pong' });
    await user.selectOptions(screen.getByLabelText('Server'), '2');
    expect(a.cartridges).toHaveBeenLastCalledWith('2');
  });

  it('keeps the dialog open with the reason when listing fails', async () => {
    const user = userEvent.setup();
    render(
      <CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={cartApi()} />,
    );
    await user.click(await screen.findByRole('button', { name: 'List a cartridge' }));
    await user.type(screen.getByLabelText('Title'), 'X');
    await user.type(screen.getByLabelText('Shelf path'), 'games/x.zip');
    await user.click(screen.getByRole('button', { name: 'List cartridge' }));
    const dialog = screen.getByRole('dialog');
    expect(
      await within(dialog).findByText(/The cartridge could not be listed/),
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText('Title')).toHaveValue('X');
  });
});

describe('D-472: Play on the cards', () => {
  it('a live built-in gets Play, which hands the game to the Start in dialog', async () => {
    const user = userEvent.setup();
    const onPlay = vi.fn();
    render(<PlayableSection flags={flags} emulatorOn={false} onPlay={onPlay} />);
    const land = screen.getByRole('article', { name: 'Land.io' });
    // A card with its own control is reached through that control, not as a whole.
    expect(land).not.toHaveAttribute('tabindex');
    await user.click(within(land).getByRole('button', { name: 'Play Land.io' }));
    expect(onPlay).toHaveBeenCalledWith({ title: 'Land.io', shelfName: 'Land.io', kind: 'seated' });
  });

  it('a browser-playable Featured pick plays by its application id; a desktop-only one has no Play', async () => {
    const user = userEvent.setup();
    const onPlay = vi.fn();
    render(
      <FeaturedSection
        featured={[
          featured(),
          featured({ application_id: '11', name: 'Horizon', eligibility: T2 }),
        ]}
        flags={flags}
        onPlay={onPlay}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Play Land.io' }));
    expect(onPlay).toHaveBeenCalledWith({
      title: 'Land.io',
      applicationId: '10',
      kind: 'seated',
    });
    expect(screen.queryByRole('button', { name: 'Play Horizon' })).toBeNull();
  });

  it('the hero shows the house picks large', () => {
    render(<ArcadeHero flags={flags} onPlay={vi.fn()} />);
    expect(screen.getByRole('heading', { level: 1, name: 'The Arcade.' })).toBeInTheDocument();
    expect(screen.getByRole('article', { name: 'Land.io' })).toHaveAttribute('data-size', 'hero');
    expect(screen.getByRole('article', { name: 'Arena 3D' })).toHaveAttribute('data-size', 'hero');
    expect(screen.queryByRole('article', { name: 'Tabletop' })).toBeNull();
  });
});

describe('D-477: the two large cards', () => {
  const pick = (over: Partial<FeaturedCard>) => featured({ blurb: null, ...over });

  it('fall back to the shipped heroes, in order, with no picks', () => {
    expect(heroSlots([]).map((s) => (s.kind === 'builtin' ? s.game.id : 'pick'))).toEqual([
      'landio',
      'arena3d',
    ]);
  });

  it('take the browser-playable picks at positions 1 and 2 first', () => {
    const one = heroSlots([pick({ application_id: 'a', name: 'Orbit', position: 0 })]);
    expect(one.map((s) => (s.kind === 'pick' ? s.pick.name : s.game.id))).toEqual([
      'Orbit',
      'landio',
    ]);
    const two = heroSlots([
      pick({ application_id: 'b', name: 'Second', position: 1 }),
      pick({ application_id: 'a', name: 'First', position: 0 }),
      pick({ application_id: 'c', name: 'Third', position: 2 }),
    ]);
    expect(two.map((s) => (s.kind === 'pick' ? s.pick.name : s.game.id))).toEqual([
      'First',
      'Second',
    ]);
  });

  it('skip a desktop-only pick (its slot goes to a built-in) and never look past position 2', () => {
    const slots = heroSlots([
      pick({ application_id: 'a', name: 'Horizon', position: 0, eligibility: T2 }),
      pick({ application_id: 'b', name: 'Orbit', position: 1 }),
      pick({ application_id: 'c', name: 'Later', position: 2 }),
    ]);
    expect(slots.map((s) => (s.kind === 'pick' ? s.pick.name : s.game.id))).toEqual([
      'Orbit',
      'landio',
    ]);
  });

  it('let a pick that is a built-in take that built-in’s slot instead of showing twice', () => {
    const byId = heroSlots(
      [pick({ application_id: '42', name: 'Land.io (v2)', position: 0 })],
      undefined,
      {
        'Land.io': '42',
      },
    );
    expect(byId.map((s) => (s.kind === 'pick' ? s.pick.name : s.game.id))).toEqual([
      'Land.io (v2)',
      'arena3d',
    ]);
    const byName = heroSlots([pick({ application_id: '9', name: 'Arena 3D', position: 0 })]);
    expect(byName.map((s) => (s.kind === 'pick' ? s.pick.name : s.game.id))).toEqual([
      'Arena 3D',
      'landio',
    ]);
  });

  it('render a pick as a large card with its blurb, corner tag and Play by id', async () => {
    const user = userEvent.setup();
    const onPlay = vi.fn();
    render(
      <ArcadeHero
        flags={flags}
        onPlay={onPlay}
        featured={[
          pick({ application_id: '77', name: 'Orbit', blurb: 'Our flagship', position: 0 }),
        ]}
      />,
    );
    const card = screen.getByRole('article', { name: 'Orbit' });
    expect(card).toHaveAttribute('data-size', 'hero');
    expect(within(card).getByText('Our flagship')).toBeInTheDocument();
    expect(within(card).getByText('Original')).toBeInTheDocument();
    await user.click(within(card).getByRole('button', { name: 'Play Orbit' }));
    expect(onPlay).toHaveBeenCalledWith({ title: 'Orbit', applicationId: '77', kind: 'seated' });
    expect(screen.getByRole('article', { name: 'Land.io' })).toHaveAttribute('data-size', 'hero');
    expect(screen.queryByRole('article', { name: 'Arena 3D' })).toBeNull();
  });
});

describe('D-479: what a member card says', () => {
  it('shows only the flags a player acts on, and never a reserved one', () => {
    expect(
      memberFlags(
        [
          'datachannel',
          'storage',
          'guest_play',
          'lobby_channel',
          'host_handoff',
          'byos_read',
          'ingame_voice',
        ],
        [{ name: 'ingame_voice', status: 'reserved', summary: '' }],
      ),
    ).toEqual(['storage', 'guest_play', 'byos_read']);
  });

  it('gives a board tool (no seats) the board kind, so Start in lists text channels first', async () => {
    const user = userEvent.setup();
    const onPlay = vi.fn();
    render(<PlayableSection flags={[]} emulatorOn={false} onPlay={onPlay} />);
    await user.click(screen.getByRole('button', { name: 'Play Whiteboard' }));
    expect(onPlay).toHaveBeenCalledWith({
      title: 'Whiteboard',
      shelfName: 'Whiteboard',
      kind: 'board',
    });
    const holdem = screen.getByRole('article', { name: 'Hold’em' });
    // Hold'em declares only plumbing: no capability line at all.
    expect(within(holdem).queryByRole('list')).toBeNull();
  });

  it('puts the tier in the corner tag and the eligibility notes on a meta line; a desktop-only pick ends in a hint', () => {
    render(
      <FeaturedSection
        featured={[
          featured({
            application_id: '11',
            name: 'Horizon',
            tier: 'TIER_2_REIMPLEMENTATION',
            eligibility: T2,
          }),
        ]}
        flags={flags}
        onPlay={vi.fn()}
      />,
    );
    const card = screen.getByRole('article', { name: 'Horizon' });
    expect(within(card).getByText('Clean-room')).toHaveClass('arc-tag');
    expect(within(card).getByText('Desktop app only')).toBeInTheDocument();
    expect(within(card).getByText('Plays in the desktop app.')).toBeInTheDocument();
    expect(within(card).queryByRole('button', { name: /Play/ })).toBeNull();
  });

  it.each([404, 500])('prints nothing when the trust badge answers %i', async (status) => {
    let asked = 0;
    server.use(
      http.get('*/api/v1/activities/:id/trust', () => {
        asked += 1;
        return new HttpResponse(null, { status });
      }),
    );
    render(
      <TrustProvider source={sandboxTrust}>
        <FeaturedSection featured={[featured()]} flags={flags} />
      </TrustProvider>,
    );
    await waitFor(() => expect(asked).toBe(1));
    const card = screen.getByRole('article', { name: 'Land.io' });
    await new Promise((r) => setTimeout(r, 20));
    expect(within(card).queryByText(/could not be loaded/)).toBeNull();
  });
});

describe('ARC-UI-010: the cover art', () => {
  it('gives a title a stable hue and an unknown kind the generic painting', () => {
    expect(hueOf('Pong')).toBe(hueOf('Pong'));
    expect(hueOf('Pong')).toBeGreaterThanOrEqual(0);
    expect(hueOf('Pong')).toBeLessThan(360);
    expect(artKind('pong')).toBe('featured');
    expect(artKind('sketch')).toBe('sketch');
    expect(artKind('holdem')).toBe('holdem');
  });

  it('is decorative and keeps its gradient ids unique per card', () => {
    const { container } = render(
      <>
        <GameArt kind="landio" />
        <GameArt kind="landio" />
      </>,
    );
    const svgs = container.querySelectorAll('svg');
    expect(svgs).toHaveLength(2);
    svgs.forEach((s) => expect(s).toHaveAttribute('aria-hidden', 'true'));
    const ids = [...container.querySelectorAll('[id]')].map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('ARC-ART-001: paints every kind as a still, decorative scene with at most one filter', () => {
    const kinds = [
      'landio',
      'arena3d',
      'holdem',
      'whiteboard',
      'tabletop',
      'sketch',
      'emulator',
      'featured',
      'cartridge',
      'workshop',
    ];
    for (const kind of kinds) {
      const { container, unmount } = render(<GameArt kind={kind} seed="Lantern Keep" />);
      const svg = container.querySelector('svg')!;
      expect(svg).toHaveAttribute('aria-hidden', 'true');
      expect(svg).toHaveAttribute('data-art', kind);
      expect(svg.querySelectorAll('filter').length).toBeLessThanOrEqual(1);
      expect(svg.querySelectorAll('animate, animateTransform, animateMotion, set')).toHaveLength(0);
      // Every url(#…) a painting uses points at a gradient or pattern inside the same painting.
      const refs = [...svg.outerHTML.matchAll(/url\(#([^)]+)\)/g)].map((m) => m[1]!);
      expect(refs.length).toBeGreaterThan(0);
      for (const ref of refs) expect(svg.querySelector(`[id="${ref}"]`)).not.toBeNull();
      unmount();
    }
  });

  it('ARC-ART-001: colours a generic scene by its title, the same on every visit', () => {
    const colours = (kind: string, seed: string) => {
      const { container, unmount } = render(<GameArt kind={kind} seed={seed} />);
      const found = [...new Set(container.innerHTML.match(/hsl\([^)]*\)/g) ?? [])].sort().join();
      unmount();
      return found;
    };
    for (const kind of ['featured', 'cartridge', 'workshop']) {
      expect(colours(kind, 'Lantern Keep')).toBe(colours(kind, 'Lantern Keep'));
      expect(colours(kind, 'Lantern Keep')).not.toBe(colours(kind, 'Moonfall Rally'));
      expect(colours(kind, 'Lantern Keep')).toContain(`hsl(${hueOf('Lantern Keep')} `);
    }
  });
});
