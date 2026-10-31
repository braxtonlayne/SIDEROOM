// BRG-SYN-003 (wave 35b): the structural half of the fake Discord (roles, channels, categories, overwrites, threads,
// forum tags, positions, audit reasons). Plain Node, built-ins only. It is mounted by fake-discord.mjs, so it listens
// on whatever port that file does (8094 for the sync-gateway agent: SR_FAKE_DISCORD_PORT=8094).
//
// REST (Discord's shapes):
//   GET  /guilds/{g}                       GET /guilds/{g}/members/{bot}
//   GET|POST|PATCH /guilds/{g}/roles       PATCH|DELETE /guilds/{g}/roles/{id}
//   GET|POST|PATCH /guilds/{g}/channels    GET /guilds/{g}/threads/active
//   GET|PATCH|DELETE /channels/{id}        POST /channels/{id}/threads
//   PUT|DELETE /channels/{id}/permissions/{target}
//
// Rules it enforces (like Discord): the bot edits only roles below its own role and grants only bits it holds unless it is
// an administrator; 250 roles / 500 channels; announcement/stage/forum/media need COMMUNITY; two name or topic changes
// per channel per ten minutes (the third is a 429); a deleted category frees its children.
//
// Every write carries X-Audit-Log-Reason (decoded, recorded in `audit`) and emits the matching gateway dispatch plus a
// GUILD_AUDIT_LOG_ENTRY_CREATE with the reason echoed, so a spec can prove echo suppression.
//
// Control routes (e2e/support/bridgeFake.ts):
//   GET  /__fake/structure                  -> { roles, channels, audit, requests }
//   POST /__fake/structure/admin            <- { method, path, body? }  a Discord admin's edit: same dispatches, no reason
//   POST /__fake/structure/control          <- { community?, premium_tier?, reset_rename_window?, rate_limit_next? }
//   POST /__fake/structure/seed             <- { roles?: [...], channels?: [...] }  objects that existed before the bridge

const ADMINISTRATOR = 1n << 3n;
const CAP_ROLES = 250;
const CAP_CHANNELS = 500;

const decodeReason = (v) => {
  try {
    return decodeURIComponent(v);
  } catch {
    return v;
  }
};

export function createStructure({ guildId, botId, channels: seedChannels, nextId, dispatch }) {
  let s;
  const fresh = () => {
    const botRole = nextId();
    s = {
      community: false,
      premiumTier: 0,
      botRole,
      roles: new Map(),
      channels: new Map(),
      audit: [],
      requests: [],
      renames: new Map(),
      rateLimitNext: 0,
    };
    s.roles.set(guildId, {
      id: guildId,
      name: '@everyone',
      color: 0,
      hoist: false,
      position: 0,
      permissions: String((1n << 10n) | (1n << 11n) | (1n << 16n)),
      managed: false,
      mentionable: false,
    });
    s.roles.set(botRole, {
      id: botRole,
      name: 'bridge-bot',
      color: 0,
      hoist: false,
      position: 1,
      permissions: String(ADMINISTRATOR),
      managed: true,
      mentionable: false,
      tags: { bot_id: botId },
    });
    seedChannels.forEach((c, i) =>
      s.channels.set(c.id, { ...c, guild_id: guildId, position: i, permission_overwrites: [] }),
    );
  };
  fresh();

  const big = (v) => {
    try {
      return BigInt(v ?? 0);
    } catch {
      return 0n;
    }
  };
  const botPerms = () => {
    let p = 0n;
    for (const id of [guildId, s.botRole]) p |= big(s.roles.get(id)?.permissions);
    return p;
  };
  const botTop = () => s.roles.get(s.botRole)?.position ?? 0;
  const mayGrant = (bits) => {
    const have = botPerms();
    return (have & ADMINISTRATOR) !== 0n || (bits & ~have) === 0n;
  };
  const overwritesOk = (list) =>
    !Array.isArray(list) || list.every((o) => mayGrant(big(o.allow) | big(o.deny)));
  const isThread = (c) => [10, 11, 12].includes(c.type);
  const communityOnly = (t) => [5, 13, 15, 16].includes(t);

  const emit = (name, data) => dispatch(name, data);
  const audit = (action, target, reason, user) => {
    s.audit.push({ action, target_id: target, reason: reason ?? null, user_id: user });
    emit('GUILD_AUDIT_LOG_ENTRY_CREATE', {
      guild_id: guildId,
      action_type: action,
      target_id: target,
      user_id: user,
      reason: reason ?? null,
    });
  };
  const ok = (status, body, headers) => ({ status, body, headers });
  const refuse = (status, code, message, headers) => ok(status, { message, code }, headers);
  const noPerms = () => refuse(403, 50013, 'Missing Permissions');

  const createRole = (b, reason, actor, check) => {
    if (s.roles.size >= CAP_ROLES)
      return refuse(400, 30005, 'Maximum number of server roles reached (250)');
    const perms = big(b.permissions);
    if (check && !mayGrant(perms)) return noPerms();
    const id = nextId();
    for (const r of s.roles.values()) if (r.position >= 1) r.position += 1;
    const role = {
      id,
      name: b.name ?? 'new role',
      color: b.color ?? 0,
      hoist: b.hoist ?? false,
      position: 1,
      permissions: String(perms),
      managed: false,
      mentionable: b.mentionable ?? false,
      unicode_emoji: b.unicode_emoji ?? null,
    };
    s.roles.set(id, role);
    emit('GUILD_ROLE_CREATE', { guild_id: guildId, role });
    audit(30, id, reason, actor);
    return ok(200, role);
  };

  const editRole = (id, b, reason, actor, check) => {
    const r = s.roles.get(id);
    if (!r) return refuse(404, 10011, 'Unknown Role');
    if (check) {
      if (id !== guildId && r.position >= botTop()) return noPerms();
      if (b.permissions !== undefined && !mayGrant(big(b.permissions))) return noPerms();
    }
    for (const k of ['name', 'permissions', 'color', 'hoist', 'mentionable', 'unicode_emoji'])
      if (b[k] !== undefined) r[k] = b[k];
    emit('GUILD_ROLE_UPDATE', { guild_id: guildId, role: r });
    audit(31, id, reason, actor);
    return ok(200, r);
  };

  const withTagIds = (tags) => tags.map((t) => (t.id == null ? { ...t, id: nextId() } : t));

  const createChannel = (b, reason, actor, check) => {
    const type = b.type ?? 0;
    if (s.channels.size >= CAP_CHANNELS)
      return refuse(400, 30005, 'Maximum number of server channels reached (500)');
    if (communityOnly(type) && !s.community)
      return refuse(400, 50024, 'Cannot execute action on this channel type');
    if (!b.name) return refuse(400, 50035, 'Invalid Form Body');
    if (check && !overwritesOk(b.permission_overwrites)) return noPerms();
    const id = nextId();
    const cap = [96000, 128000, 256000, 384000][Math.min(s.premiumTier, 3)];
    const c = {
      id,
      guild_id: guildId,
      type,
      position: b.position ?? s.channels.size,
      permission_overwrites: b.permission_overwrites ?? [],
    };
    for (const k of [
      'name',
      'topic',
      'parent_id',
      'nsfw',
      'rate_limit_per_user',
      'user_limit',
      'video_quality_mode',
      'default_reaction_emoji',
      'default_sort_order',
      'default_forum_layout',
      'default_auto_archive_duration',
      'default_thread_rate_limit_per_user',
    ])
      if (b[k] !== undefined) c[k] = b[k];
    if (b.bitrate !== undefined) c.bitrate = Math.min(b.bitrate, cap);
    if (Array.isArray(b.available_tags)) c.available_tags = withTagIds(b.available_tags);
    s.channels.set(id, c);
    emit('CHANNEL_CREATE', c);
    audit(10, id, reason, actor);
    return ok(200, c);
  };

  const editChannel = (id, b, reason, actor, check) => {
    const c = s.channels.get(id);
    if (!c) return refuse(404, 10003, 'Unknown Channel');
    if (check && !overwritesOk(b.permission_overwrites)) return noPerms();
    const renames = ['name', 'topic'].some((k) => b[k] !== undefined && b[k] !== c[k]);
    if (renames && !isThread(c)) {
      const n = s.renames.get(id) ?? 0;
      if (n >= 2) return refuse(429, 0, 'You are being rate limited.', { 'retry-after': '300' });
      s.renames.set(id, n + 1);
    }
    for (const [k, v] of Object.entries(b)) {
      if (['archived', 'locked', 'auto_archive_duration', 'invitable'].includes(k)) {
        c.thread_metadata = { ...(c.thread_metadata ?? {}), [k]: v };
      } else if (k === 'available_tags' && Array.isArray(v)) c.available_tags = withTagIds(v);
      else if (k !== 'type') c[k] = v;
    }
    emit(isThread(c) ? 'THREAD_UPDATE' : 'CHANNEL_UPDATE', c);
    audit(11, id, reason, actor);
    return ok(200, c);
  };

  const createThread = (parent, b, reason, actor) => {
    const id = nextId();
    const t = {
      id,
      guild_id: guildId,
      type: b.type ?? 11,
      name: b.name,
      parent_id: parent,
      owner_id: actor,
      applied_tags: b.applied_tags ?? [],
      thread_metadata: {
        archived: false,
        locked: false,
        auto_archive_duration: b.auto_archive_duration ?? 1440,
        invitable: b.invitable ?? true,
      },
    };
    s.channels.set(id, t);
    emit('THREAD_CREATE', t);
    audit(110, id, reason, actor);
    return ok(201, t);
  };

  /** Returns { status, body, headers } or null when the route is not structural. */
  function route(method, segs, body, reason, actor, check) {
    const g = guildId;
    const n = segs.length;
    if (segs[0] === 'guilds' && segs[1] === g) {
      if (n === 2 && method === 'GET')
        return ok(200, {
          id: g,
          name: 'Fake Discord',
          owner_id: '1',
          premium_tier: s.premiumTier,
          features: s.community ? ['COMMUNITY'] : [],
        });
      if (n === 4 && segs[2] === 'members' && method === 'GET')
        return segs[3] === botId
          ? ok(200, { user: { id: botId, bot: true }, roles: [s.botRole] })
          : refuse(404, 10007, 'Unknown Member');
      if (segs[2] === 'roles') {
        if (n === 3 && method === 'GET') return ok(200, [...s.roles.values()]);
        if (n === 3 && method === 'POST') return createRole(body, reason, actor, check);
        if (n === 3 && method === 'PATCH') {
          if (!Array.isArray(body)) return refuse(400, 50035, 'Invalid Form Body');
          if (
            check &&
            body.some(
              (e) => (s.roles.get(e.id)?.position ?? 99) >= botTop() || e.position >= botTop(),
            )
          )
            return noPerms();
          for (const e of body) if (s.roles.has(e.id)) s.roles.get(e.id).position = e.position;
          for (const e of body) emit('GUILD_ROLE_UPDATE', { guild_id: g, role: s.roles.get(e.id) });
          return ok(200, [...s.roles.values()]);
        }
        if (n === 4 && method === 'PATCH') return editRole(segs[3], body, reason, actor, check);
        if (n === 4 && method === 'DELETE') {
          const r = s.roles.get(segs[3]);
          if (!r) return refuse(404, 10011, 'Unknown Role');
          if (segs[3] === g || (check && r.position >= botTop())) return noPerms();
          s.roles.delete(segs[3]);
          emit('GUILD_ROLE_DELETE', { guild_id: g, role_id: segs[3] });
          audit(32, segs[3], reason, actor);
          return { status: 204 };
        }
      }
      if (segs[2] === 'channels' && n === 3) {
        if (method === 'GET')
          return ok(
            200,
            [...s.channels.values()].filter((c) => !isThread(c)),
          );
        if (method === 'POST') return createChannel(body, reason, actor, check);
        if (method === 'PATCH' && Array.isArray(body)) {
          for (const e of body) {
            const c = s.channels.get(e.id);
            if (!c) continue;
            if (e.position !== undefined) c.position = e.position;
            if ('parent_id' in e) c.parent_id = e.parent_id;
            emit('CHANNEL_UPDATE', c);
          }
          return { status: 204 };
        }
      }
      if (segs[2] === 'threads' && segs[3] === 'active' && method === 'GET')
        return ok(200, {
          threads: [...s.channels.values()].filter(
            (c) => isThread(c) && !c.thread_metadata?.archived,
          ),
          members: [],
        });
      return null;
    }
    if (segs[0] === 'channels') {
      const id = segs[1];
      if (n === 2 && method === 'GET')
        return s.channels.has(id)
          ? ok(200, s.channels.get(id))
          : refuse(404, 10003, 'Unknown Channel');
      if (n === 2 && method === 'PATCH') return editChannel(id, body, reason, actor, check);
      if (n === 2 && method === 'DELETE') {
        const c = s.channels.get(id);
        if (!c) return refuse(404, 10003, 'Unknown Channel');
        s.channels.delete(id);
        if (c.type === 4)
          for (const k of s.channels.values())
            if (k.parent_id === id) {
              k.parent_id = null;
              emit('CHANNEL_UPDATE', k);
            }
        emit(isThread(c) ? 'THREAD_DELETE' : 'CHANNEL_DELETE', c);
        audit(12, id, reason, actor);
        return ok(200, c);
      }
      if (n === 4 && segs[2] === 'permissions') {
        const c = s.channels.get(id);
        if (!c) return refuse(404, 10003, 'Unknown Channel');
        if (method === 'PUT') {
          const ow = { id: segs[3], type: body.type, allow: body.allow, deny: body.deny };
          if (check && !overwritesOk([ow])) return noPerms();
          c.permission_overwrites = [
            ...c.permission_overwrites.filter((o) => o.id !== segs[3]),
            ow,
          ];
          emit('CHANNEL_UPDATE', c);
          audit(14, id, reason, actor);
          return { status: 204 };
        }
        if (method === 'DELETE') {
          c.permission_overwrites = c.permission_overwrites.filter((o) => o.id !== segs[3]);
          emit('CHANNEL_UPDATE', c);
          audit(15, id, reason, actor);
          return { status: 204 };
        }
      }
      if (n === 3 && segs[2] === 'threads' && method === 'POST') {
        if (!s.channels.has(id)) return refuse(404, 10003, 'Unknown Channel');
        return createThread(id, body, reason, actor);
      }
    }
    return null;
  }

  return {
    reset: fresh,
    channelExists: (id) => s.channels.has(id),
    /** A webhook post's destination: a thread_id query, or a new forum post (thread_name). */
    webhookTarget(channelId, query, body, hookId) {
      let target = query.get('thread_id') ?? channelId;
      if (body.thread_name) {
        const r = createThread(
          channelId,
          { name: body.thread_name, applied_tags: body.applied_tags },
          null,
          hookId,
        );
        target = r.body.id;
      }
      return target;
    },
    /** Handle a bot REST call; null when not structural. Async-free: the body is already parsed. */
    handleRest(method, segs, body, headers, auth) {
      if (!auth) return refuse(401, 0, '401: Unauthorized');
      s.requests.push({
        method,
        path: '/' + segs.join('/'),
        body,
        reason: headers['x-audit-log-reason'] ? decodeReason(headers['x-audit-log-reason']) : null,
      });
      const reason = headers['x-audit-log-reason']
        ? decodeReason(headers['x-audit-log-reason'])
        : null;
      if (s.rateLimitNext > 0) {
        s.rateLimitNext -= 1;
        return refuse(429, 0, 'You are being rate limited.', { 'retry-after': '1' });
      }
      return route(method, segs, body, reason, botId, true);
    },
    /** Control routes. Returns { status, body } or null. */
    handleControl(method, path, body) {
      if (path === '/__fake/structure' && method === 'GET')
        return ok(200, {
          roles: [...s.roles.values()],
          channels: [...s.channels.values()],
          audit: s.audit,
          requests: s.requests,
        });
      if (path === '/__fake/structure/admin' && method === 'POST') {
        const segs = String(body.path ?? '')
          .replace(/^\/api\/v\d+\//, '')
          .split('/')
          .filter(Boolean);
        const r = route(
          String(body.method ?? 'GET').toUpperCase(),
          segs,
          body.body ?? {},
          null,
          body.user_id ?? '9100000000000000042',
          false,
        );
        return r ?? refuse(404, 0, 'Unknown route');
      }
      if (path === '/__fake/structure/control' && method === 'POST') {
        if (typeof body.community === 'boolean') s.community = body.community;
        if (typeof body.premium_tier === 'number') s.premiumTier = body.premium_tier;
        if (body.reset_rename_window) s.renames.clear();
        if (typeof body.rate_limit_next === 'number') s.rateLimitNext = body.rate_limit_next;
        return ok(200, { ok: true });
      }
      if (path === '/__fake/structure/seed' && method === 'POST') {
        for (const r of body.roles ?? []) {
          const id = r.id ?? nextId();
          s.roles.set(id, {
            color: 0,
            hoist: false,
            managed: false,
            mentionable: false,
            permissions: '0',
            position: 2,
            ...r,
            id,
          });
        }
        for (const c of body.channels ?? []) {
          const id = c.id ?? nextId();
          s.channels.set(id, {
            guild_id: guildId,
            position: s.channels.size,
            permission_overwrites: [],
            ...c,
            id,
          });
        }
        return ok(200, { ok: true });
      }
      return null;
    },
    /** For GUILD_CREATE. */
    snapshot: () => ({
      roles: [...s.roles.values()],
      channels: [...s.channels.values()].filter((c) => !isThread(c)),
    }),
  };
}
