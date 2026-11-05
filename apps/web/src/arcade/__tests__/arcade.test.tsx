// ARC-ROOM-001: the Arcade sections (Featured, Guild Cartridges, Coming Soon), tip badges and the trust seam.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useEffect, type ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { ArcadeApi } from '../api';
import {
  CartridgesSection,
  ComingSoonSection,
  FeaturedSection,
  PlayableSection,
} from '../ArcadeSections';
import { builtinsByStatus, type BuiltinGame } from '../builtins';
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
    expect(within(land).getByText('Guest play')).toBeInTheDocument();
    expect(within(land).getByText('(planned)')).toBeInTheDocument();
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
    render(<PlayableSection flags={flags} emulatorOn />);
    const region = screen.getByRole('region', { name: 'Playable now' });
    for (const name of ['Land.io', 'Arena 3D', 'Hold’em', 'Whiteboard', 'Tabletop']) {
      const card = within(region).getByRole('article', { name });
      expect(card.querySelector('svg[data-art]')).not.toBeNull();
      // Reachable by Tab although it has no button of its own.
      expect(card).toHaveAttribute('tabindex', '0');
      expect(within(card).getByText('Playable now')).toBeInTheDocument();
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
      { id: 'landio', packageId: 'p1', status: 'live', capabilities: [], art: 'landio' },
      { id: 'whiteboard', packageId: 'p2', status: 'preview', capabilities: [], art: 'nope' },
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

describe('ARC-UI-010: the cover art', () => {
  it('gives a title a stable hue and an unknown kind the generic painting', () => {
    expect(hueOf('Pong')).toBe(hueOf('Pong'));
    expect(hueOf('Pong')).toBeGreaterThanOrEqual(0);
    expect(hueOf('Pong')).toBeLessThan(360);
    expect(artKind('sketch')).toBe('featured');
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
});
