import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { loadPermissions } from '@sideroom/wasm';
import { beforeAll, describe, expect, it } from 'vitest';
import type { PermEngine } from '../../../lounge/permEngine';
import type { ChannelObject, RoleObject } from '../api';
import { hasAdministrator, viewServerAs } from '../viewAs';
import { bits, restRole } from './helpers';

// J-09 (LNG-SSET-036): the visibility comes from the shared Rust engine. Needs `pnpm --filter @sideroom/wasm build`.
// vitest runs with apps/web as the working directory.
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/permissions/permissions_bg.wasm');
const built = existsSync(wasmPath);
if (!built && process.env.SR_WASM_REQUIRED === '1') throw new Error(`missing ${wasmPath}`);

const VIEW = 10;
const ADMIN = 3;

const ch = (id: string, name: string, type: number, over: Partial<ChannelObject> = {}) =>
  ({
    id,
    type,
    guild_id: '100',
    name,
    topic: null,
    position: 0,
    parent_id: null,
    nsfw: false,
    rate_limit_per_user: 0,
    permission_overwrites: [],
    ...over,
  }) as ChannelObject;

const roles: RoleObject[] = [
  restRole('100', '@everyone', 0, { permissions: bits(VIEW, 11) }) as RoleObject,
  restRole('201', 'Mods', 2, { permissions: bits(1) }) as RoleObject,
  restRole('202', 'Boss', 3, { permissions: bits(ADMIN) }) as RoleObject,
];
const channels: ChannelObject[] = [
  ch('301', '301', 0),
  ch('302', 'Staff', 4, { position: 1 }),
  // Denied to @everyone, allowed back for Mods.
  ch('303', 'staff-room', 0, {
    parent_id: '302',
    position: 0,
    permission_overwrites: [
      { id: '100', type: 0, allow: '0', deny: bits(VIEW) },
      { id: '201', type: 0, allow: bits(VIEW), deny: '0' },
    ],
  }),
  ch('304', '304', 0, {
    parent_id: '302',
    position: 1,
    permission_overwrites: [{ id: '100', type: 0, allow: '0', deny: bits(VIEW) }],
  }),
  ch('305', 'Hangout', 2, { position: 2 }),
];

describe.skipIf(!built)('View Server As Role (J-09)', () => {
  let engine: PermEngine;
  beforeAll(async () => {
    const api = await loadPermissions(readFileSync(wasmPath));
    engine = { compute: (i) => api.compute(i) };
  });

  const seen = (picked: string[]) =>
    viewServerAs(engine, { guildId: '100', roles, picked: new Set(picked), channels })!
      .filter((e) => e.visible)
      .map((e) => e.channel.id);

  it('shows only what @everyone sees when no role is picked: a deny overwrite hides a channel', () => {
    expect(seen([])).toEqual(['301', '305']);
  });

  it('a role that an overwrite allows back brings the channel and its category', () => {
    expect(seen(['201'])).toEqual(['301', '305', '302', '303']);
    expect(seen(['201'])).not.toContain('304');
  });

  it('Administrator sees every channel', () => {
    expect(seen(['202'])).toEqual(['301', '305', '302', '303', '304']);
    expect(hasAdministrator(roles, new Set(['202']))).toBe(true);
    expect(hasAdministrator(roles, new Set(['201']))).toBe(false);
  });

  it('lists categories before their channels and loose channels first', () => {
    const all = viewServerAs(engine, {
      guildId: '100',
      roles,
      picked: new Set(['202']),
      channels,
    })!;
    expect(all.map((e) => e.channel.id)).toEqual(['301', '305', '302', '303', '304']);
  });
});
