// ARC-ROOM-001: the Arcade sections (Featured, Guild Cartridges, Coming Soon), tip badges and the trust seam.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ArcadeApi } from '../api';
import { CartridgesSection, ComingSoonSection, FeaturedSection } from '../ArcadeSections';
import { TipBadges } from '../TipBadges';
import { TrustProvider } from '../trust';
import type {
  CapabilityFlag,
  Cartridge,
  ComingSoonCard,
  Eligibility,
  FeaturedCard,
} from '../types';

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
        emulatorOn={false}
      />,
    );
    expect(screen.getByRole('heading', { name: 'Featured' })).toBeInTheDocument();
    const land = screen.getByRole('heading', { name: 'Land.io' }).closest('article')!;
    expect(within(land).getByText('Original')).toBeInTheDocument();
    expect(within(land).getByText('guest_play')).toBeInTheDocument();
    expect(within(land).getByText('(planned)')).toBeInTheDocument();
    expect(within(land).getByRole('link', { name: 'Ko-fi' })).toHaveAttribute(
      'href',
      'https://ko-fi.com/dev',
    );
    const horizon = screen.getByRole('heading', { name: 'Horizon' }).closest('article')!;
    expect(within(horizon).getByText('Desktop app only')).toBeInTheDocument();
    expect(within(horizon).queryByText(/voice channel/)).toBeNull();
  });

  it('says so when nothing is featured', () => {
    render(<FeaturedSection featured={[]} flags={flags} emulatorOn={false} />);
    expect(screen.getByText(/Nothing is featured yet/)).toBeInTheDocument();
  });

  it('shows the badges and actions the trust provider supplies for a build hash', () => {
    const badges = vi.fn(() => [{ id: 'sig', label: 'SWARM-SAFETY-SIGNED', tone: 'good' as const }]);
    render(
      <TrustProvider source={{ badges, actions: (c) => <button>Vouch {c.name}</button> }}>
        <FeaturedSection featured={[featured()]} flags={flags} emulatorOn={false} />
      </TrustProvider>,
    );
    expect(screen.getByText('SWARM-SAFETY-SIGNED')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Vouch Land.io' })).toBeInTheDocument();
    expect(badges).toHaveBeenCalledWith({
      applicationId: '10',
      name: 'Land.io',
      bundleSha256: 'a'.repeat(64),
    });
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
    render(<CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={api()} />);
    expect(await screen.findByRole('heading', { name: 'Pong' })).toBeInTheDocument();
    const tier = screen.getByLabelText('Content tier') as HTMLSelectElement;
    expect([...tier.options].map((o) => o.value)).toEqual([
      'TIER_1_ORIGINAL',
      'TIER_2_REIMPLEMENTATION',
    ]);
  });

  it('lists and unlists a cartridge', async () => {
    const user = userEvent.setup();
    const a = api();
    render(<CartridgesSection guilds={[{ id: '1', name: 'Friends' }]} flags={flags} api={a} />);
    await screen.findByRole('heading', { name: 'Pong' });
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
    await user.click(screen.getByRole('button', { name: 'Unlist Pong' }));
    expect(a.removeCartridge).toHaveBeenCalledWith('1', '7');
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Pong' })).toBeNull());
  });

  it('says so when the person is in no server', () => {
    render(<CartridgesSection guilds={[]} flags={flags} api={api()} />);
    expect(screen.getByText('You are not in a server yet.')).toBeInTheDocument();
  });
});
