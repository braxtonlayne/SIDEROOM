// OPS-FLG-001: Admin > Features and Guild Settings > Modules render every feature of the registry, from the API
// list, with no hand-made toggle. The list is the Rust registry itself: `crates/sr-server/tests/feature_registry.rs`
// writes `__fixtures__/registry.json` and fails when it is stale, so a feature added to the registry shows up here
// without anyone editing this test.
import { screen, within } from '@testing-library/react';
import fs from 'node:fs';
import path from 'node:path';
import { TOOLS } from '../../../tools/registry';
import { asStaff, http, HttpResponse, openAdmin, server } from '../../__tests__/helpers';
import { MANAGE, openSettings, seed } from '../../../settings/server/__tests__/helpers';
import catalog from '../__fixtures__/registry.json';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

interface Entry {
  id: string;
  name: string;
  description: string;
  section: string;
  phase: number;
  default_enabled: boolean;
  scopes: string[];
  client_visible: boolean;
  owner_only: boolean;
  operator_gated: boolean;
  gated_by: string | null;
}
const features = catalog as Entry[];

/** The ids the roadmap names for OPS-FLG-001, with the shipped id where it differs. */
const ROADMAP: [string, string][] = [
  ['platform.byos', 'platform.byos'],
  ['lounge.scrapbook', 'lounge.scrapbook'],
  ['platform.public_profiles', 'platform.public_profiles'],
  ['platform.bridge', 'platform.bridge'],
  ['bridge.enabled', 'bridge.enabled'],
  ['lounge.jukebox', 'lounge.jukebox'],
  ['lounge.listening_rooms', 'lounge.listening_rooms'],
  ['platform.tools', 'platform.tools'],
  ['platform.activities_datachannel', 'platform.activities_datachannel'],
  ['platform.emulator', 'platform.emulator'],
  ['lounge.status_rings', 'lounge.rings'],
  ['platform.watch_together_embeds', 'platform.watch_together_embeds'],
  ['platform.game_servers', 'platform.game_servers'],
];

const adminView = (f: Entry) => ({
  id: f.id,
  name: f.name,
  description: f.description,
  section: f.section,
  phase: f.phase,
  scopes: f.scopes,
  enabled: f.default_enabled,
  default_enabled: f.default_enabled,
  schema: { fields: [] },
  settings: {},
});

const guildView = (f: Entry) => ({
  id: f.id,
  name: f.name,
  description: f.description,
  section: f.section,
  phase: f.phase,
  enabled: f.default_enabled,
  default_enabled: f.default_enabled,
  platform_disabled: false,
  owner_only: f.owner_only,
  settings: {},
  schema: { fields: [] },
});

describe('the registry the two screens render', () => {
  it('holds every id the roadmap names, with the scopes that make sense', () => {
    const byId = new Map(features.map((f) => [f.id, f]));
    for (const [, shipped] of ROADMAP) expect(byId.has(shipped), shipped).toBe(true);
    // Both scopes where a server may choose; platform only where it is the operator's call.
    for (const id of [
      'platform.byos',
      'lounge.scrapbook',
      'lounge.jukebox',
      'lounge.listening_rooms',
    ]) {
      expect(byId.get(id)!.scopes, id).toEqual(['platform', 'guild']);
    }
    expect(byId.get('bridge.enabled')!.scopes).toEqual(['guild']);
    expect(byId.get('bridge.enabled')!.owner_only).toBe(true);
    expect(byId.get('bridge.enabled')!.default_enabled).toBe(false);
    for (const id of [
      'platform.public_profiles',
      'platform.bridge',
      'platform.tools',
      'platform.activities_datachannel',
      'platform.emulator',
      'platform.watch_together_embeds',
      'lounge.rings',
    ]) {
      expect(byId.get(id)!.scopes, id).toEqual(['platform']);
    }
  });

  it('has one sub-flag for each tool of the drawer except its own guide', () => {
    const subs = features
      .filter((f) => f.id.startsWith('platform.tools.'))
      .map((f) => f.id.slice('platform.tools.'.length))
      .sort();
    const tools = TOOLS.map((t) => t.id)
      .filter((id) => id !== 'guide')
      .map((id) => id.replace(/-/g, '_'))
      .sort();
    expect(subs).toEqual(tools);
    // The Rust list the registry is built from says the same (it is the source of the fixture).
    const rust = fs.readFileSync(
      path.resolve(__dirname, '../../../../../../crates/sr-features/src/wave_modules.rs'),
      'utf8',
    );
    const block = rust.slice(
      rust.indexOf('// tool-flags:begin'),
      rust.indexOf('// tool-flags:end'),
    );
    const listed = [...block.matchAll(/\("([a-z0-9_]+)", "/g)].map((m) => m[1]!).sort();
    expect(listed).toEqual(tools);
  });
});

describe('Admin > Features', () => {
  it('renders every registered feature, each with its own switch', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    server.use(
      http.get('*/api/v1/admin/features', () => HttpResponse.json(features.map(adminView))),
    );
    openAdmin('features');
    const list = await screen.findByRole('list', { name: 'Feature modules' });
    const rows = within(list).getAllByRole('listitem');
    expect(rows).toHaveLength(features.length);
    expect(within(list).getAllByRole('switch')).toHaveLength(features.length);
    // Every description is on screen (the roadmap's "no hand-made toggle": the screen knows nothing else).
    for (const f of features) {
      expect(within(list).getAllByText(f.description).length, f.id).toBeGreaterThan(0);
    }
    // Names are not unique (the bridge has a platform switch and a server module), so count by name.
    const names = new Map<string, number>();
    for (const f of features) names.set(f.name, (names.get(f.name) ?? 0) + 1);
    for (const [name, n] of names) {
      expect(within(list).getAllByRole('switch', { name: `${name} enabled` }), name).toHaveLength(
        n,
      );
    }
    // The ids of this wave are among them.
    for (const [, shipped] of ROADMAP) {
      const f = features.find((x) => x.id === shipped)!;
      expect(within(list).getAllByText(f.description).length).toBeGreaterThan(0);
    }
  });
});

describe('Guild Settings > Modules', () => {
  it('renders every feature a server may switch, and none that only the platform owns', async () => {
    seed({ perms: [MANAGE.guild] });
    // What the API lists for a server: the guild-scope features (an operator-gated one only once offered, which
    // the operator did here).
    const forGuild = features.filter((f) => f.scopes.includes('guild'));
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json(forGuild.map(guildView))),
    );
    await openSettings('features');
    expect(await screen.findByRole('heading', { name: 'Voice jukebox' })).toBeInTheDocument();
    for (const f of forGuild) {
      if (f.id === 'bridge.enabled') {
        // The one owner-only module has its own card, still built from the registry entry.
        const card = screen.getByTestId('bridge-card');
        expect(within(card).getByRole('switch', { name: `Enable ${f.name}` })).toBeInTheDocument();
        continue;
      }
      expect(screen.getByRole('heading', { name: f.name, level: 3 }), f.id).toBeInTheDocument();
      expect(screen.getByRole('switch', { name: `Enable ${f.name}` }), f.id).toBeInTheDocument();
    }
    // Platform-only switches are the operator's; the server screen never shows them. (The bridge's platform switch
    // shares its name with the server module, so names the server screen does show are skipped.)
    const guildNames = new Set(forGuild.map((f) => f.name));
    for (const f of features.filter(
      (x) => !x.scopes.includes('guild') && !guildNames.has(x.name),
    )) {
      expect(screen.queryByRole('heading', { name: f.name, level: 3 }), f.id).toBeNull();
    }
  });
});
