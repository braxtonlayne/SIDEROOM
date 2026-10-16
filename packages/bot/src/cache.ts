import type { Channel, Guild, Ready, Role, User } from './types';

/** What the gateway has told the bot: itself, guilds, channels, roles. */
export class Cache {
  user: User | undefined;
  applicationId: string | undefined;
  readonly guilds = new Map<string, Guild>();

  channel(id: string): Channel | undefined {
    for (const g of this.guilds.values()) {
      const c = g.channels?.find((c) => c.id === id);
      if (c) return c;
    }
    return undefined;
  }

  role(guildId: string, roleId: string): Role | undefined {
    return this.guilds.get(guildId)?.roles?.find((r) => r.id === roleId);
  }

  /** Fold one dispatch into the cache. Unknown events and malformed payloads are ignored. */
  apply(name: string, data: unknown): void {
    const d = (data ?? {}) as Record<string, unknown>;
    switch (name) {
      case 'READY': {
        const r = d as unknown as Ready;
        this.user = r.user;
        this.applicationId = r.application?.id;
        this.guilds.clear();
        for (const g of r.guilds ?? []) this.guilds.set(g.id, g);
        return;
      }
      case 'GUILD_CREATE': {
        const g = d as unknown as Guild;
        if (typeof g.id === 'string') this.guilds.set(g.id, g);
        return;
      }
      case 'GUILD_DELETE':
        if (typeof d.id === 'string') this.guilds.delete(d.id);
        return;
      case 'CHANNEL_CREATE':
      case 'CHANNEL_UPDATE': {
        const c = d as unknown as Channel;
        const g = c.guild_id ? this.guilds.get(c.guild_id) : undefined;
        if (g) g.channels = upsert(g.channels ?? [], c);
        return;
      }
      case 'CHANNEL_DELETE': {
        const g = typeof d.guild_id === 'string' ? this.guilds.get(d.guild_id) : undefined;
        if (g) g.channels = (g.channels ?? []).filter((c) => c.id !== d.id);
        return;
      }
      case 'GUILD_ROLE_CREATE':
      case 'GUILD_ROLE_UPDATE': {
        const g = typeof d.guild_id === 'string' ? this.guilds.get(d.guild_id) : undefined;
        const role = d.role as Role | undefined;
        if (g && role) g.roles = upsert(g.roles ?? [], role);
        return;
      }
      case 'GUILD_ROLE_DELETE': {
        const g = typeof d.guild_id === 'string' ? this.guilds.get(d.guild_id) : undefined;
        if (g) g.roles = (g.roles ?? []).filter((r) => r.id !== d.role_id);
        return;
      }
    }
  }
}

function upsert<T extends { id: string }>(list: T[], item: T): T[] {
  const i = list.findIndex((x) => x.id === item.id);
  if (i < 0) return [...list, item];
  const copy = list.slice();
  copy[i] = item;
  return copy;
}
