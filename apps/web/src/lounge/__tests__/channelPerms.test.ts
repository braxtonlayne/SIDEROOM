import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { loadPermissions } from '@sideroom/wasm';
import { afterEach } from 'vitest';
import { buildLiveSnapshot, type SnapshotInput } from '../live';
import { setPermEngine, type PermEngine } from '../permEngine';
import type * as S from '../../store/types';

// F-17 (D-178): channel permissions come from the shared Rust engine. Needs `pnpm --filter @sideroom/wasm build`.
// vitest runs with apps/web as the working directory.
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/permissions/permissions_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') throw new Error(`missing ${wasmPath}`);

const VIEW = 1n << 10n;
const SEND = 1n << 11n;
const MANAGE_MESSAGES = 1n << 13n;
const PIN = 1n << 51n;
const str = (n: bigint) => n.toString();

const me: S.User = { id: '1', username: 'me' };
const role = (id: string, permissions: bigint, position = 0): S.Role => ({
  id,
  guild_id: '10',
  name: id,
  color: 0,
  position,
  permissions: str(permissions),
});
const chan = (id: string, overwrites: S.Channel['permission_overwrites'] = []): S.Channel => ({
  id,
  type: 0,
  guild_id: '10',
  name: id,
  position: Number(id.slice(1)),
  permission_overwrites: overwrites,
});

function input(extra: Partial<SnapshotInput> = {}): SnapshotInput {
  return {
    currentUserId: '1',
    users: { '1': me },
    guilds: { 10: { id: '10', name: 'One', owner_id: '9' } },
    guildOrder: ['10'],
    channels: {
      c1: chan('c1'),
      // The mod role may pin everywhere except here.
      c2: chan('c2', [{ id: '20', type: 0, allow: '0', deny: str(PIN) }]),
      // @everyone cannot send here.
      c3: chan('c3', [{ id: '10', type: 0, allow: '0', deny: str(SEND) }]),
    },
    channelIdsByGuild: { 10: ['c1', 'c2', 'c3'] },
    roles: { 10: role('10', VIEW | SEND), '20': role('20', PIN | MANAGE_MESSAGES, 1) },
    roleIdsByGuild: { 10: ['10', '20'] },
    members: { '10:1': { guild_id: '10', user_id: '1', roles: ['20'] } },
    memberIdsByGuild: { 10: ['1'] },
    presences: {},
    readStates: {},
    ...extra,
  };
}

afterEach(() => setPermEngine(null));

describe('channel permissions without the engine', () => {
  it('falls back to the guild level and leaves channel.permissions unset', () => {
    const snap = buildLiveSnapshot(input());
    expect(snap.channelsByGuild['10']![1]!.permissions).toBeUndefined();
    // Guild level: PIN_MESSAGES alone grants pinning, regardless of overwrites.
    expect(snap.guilds[0]!.permissions.pinMessages).toBe(true);
  });

  it('does not treat Manage Messages as Pin Messages (D-178)', () => {
    const snap = buildLiveSnapshot(
      input({ roles: { 10: role('10', VIEW | SEND), '20': role('20', MANAGE_MESSAGES, 1) } }),
    );
    expect(snap.guilds[0]!.permissions.manageMessages).toBe(true);
    expect(snap.guilds[0]!.permissions.pinMessages).toBe(false);
  });
});

describe('channel permissions with an injected engine', () => {
  it('passes roles, overwrites and the timeout to the engine', () => {
    const seen: unknown[] = [];
    const engine: PermEngine = {
      compute: (i) => {
        seen.push(i);
        return str(VIEW | SEND);
      },
    };
    const until = '2030-01-01T00:00:00.000Z';
    buildLiveSnapshot(
      input({
        engine,
        members: {
          '10:1': {
            guild_id: '10',
            user_id: '1',
            roles: ['10', '20'],
            communication_disabled_until: until,
          },
        },
      }),
    );
    expect(seen).toHaveLength(3);
    expect(seen[1]).toMatchObject({
      is_owner: false,
      everyone: str(VIEW | SEND),
      role_permissions: [str(PIN | MANAGE_MESSAGES)],
      member_role_ids: ['20'],
      overwrites: [{ id: '20', kind: 'role', allow: '0', deny: str(PIN) }],
      communication_disabled_until_ms: Date.parse(until),
    });
  });

  it('keeps the fallback while the viewer member is not loaded', () => {
    const engine: PermEngine = { compute: () => '0' };
    const snap = buildLiveSnapshot(input({ engine, members: {}, memberIdsByGuild: {} }));
    expect(snap.channelsByGuild['10']![0]!.permissions).toBeUndefined();
    expect(snap.channelsByGuild['10']![0]!.canSend).toBe(true);
  });
});

describe.runIf(built)('channel permissions with the real Rust engine', () => {
  async function engine(): Promise<PermEngine> {
    const api = await loadPermissions(readFileSync(wasmPath));
    return { compute: (i) => api.compute(i) };
  }

  it('an overwrite that denies pin in one channel hides pinning only there', async () => {
    const snap = buildLiveSnapshot(input({ engine: await engine() }));
    const [c1, c2] = snap.channelsByGuild['10']!;
    expect(c1!.permissions).toMatchObject({ pinMessages: true, manageMessages: true, send: true });
    expect(c2!.permissions).toMatchObject({ pinMessages: false, manageMessages: true });
  });

  it('an @everyone send overwrite locks the composer in that channel', async () => {
    const snap = buildLiveSnapshot(input({ engine: await engine() }));
    const c3 = snap.channelsByGuild['10']![2]!;
    expect(c3.permissions!.send).toBe(false);
    expect(c3.canSend).toBe(false);
    expect(snap.channelsByGuild['10']![0]!.canSend).toBe(true);
  });

  it('a member overwrite re-allows what the role overwrite denied', async () => {
    const channels = {
      c1: chan('c1', [
        { id: '20', type: 0, allow: '0', deny: str(PIN) },
        { id: '1', type: 1, allow: str(PIN), deny: '0' },
      ]),
    };
    const snap = buildLiveSnapshot(
      input({ engine: await engine(), channels, channelIdsByGuild: { 10: ['c1'] } }),
    );
    expect(snap.channelsByGuild['10']![0]!.permissions!.pinMessages).toBe(true);
  });

  it('an active timeout removes send but keeps reading', async () => {
    const future = new Date(Date.now() + 3_600_000).toISOString();
    const snap = buildLiveSnapshot(
      input({
        engine: await engine(),
        members: {
          '10:1': {
            guild_id: '10',
            user_id: '1',
            roles: ['20'],
            communication_disabled_until: future,
          },
        },
      }),
    );
    expect(snap.channelsByGuild['10']![0]!.permissions).toMatchObject({ view: true, send: false });
  });

  it('the owner has every permission in every channel', async () => {
    const snap = buildLiveSnapshot(
      input({
        engine: await engine(),
        guilds: { 10: { id: '10', name: 'One', owner_id: '1' } },
      }),
    );
    expect(snap.channelsByGuild['10']![1]!.permissions!.pinMessages).toBe(true);
    expect(snap.channelsByGuild['10']![2]!.permissions!.send).toBe(true);
  });
});
