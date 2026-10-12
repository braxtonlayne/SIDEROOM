import { dmTitle, HOME, isTextLike, isVoiceLike } from './channels';
import type { Channel, DmChannel, Guild, LoungeUser, Snowflake } from './data';

export type SwitcherKind = 'dm' | 'user' | 'text' | 'voice' | 'guild';

export interface SwitcherItem {
  key: string;
  kind: SwitcherKind;
  label: string;
  hint: string;
  guildId: Snowflake;
  channelId?: Snowflake;
}

export interface SwitcherSources {
  guilds: readonly Guild[];
  channelsByGuild: Record<Snowflake, readonly Channel[]>;
  dms: readonly DmChannel[];
  users: Record<Snowflake, LoungeUser>;
  meId: Snowflake;
}

export function buildSwitcherItems(src: SwitcherSources): SwitcherItem[] {
  const items: SwitcherItem[] = [];
  const nameOf = (id: Snowflake) => src.users[id]?.displayName ?? 'Unknown';
  for (const d of src.dms) {
    items.push({
      key: `dm:${d.id}`,
      kind: 'dm',
      label: dmTitle(d, nameOf),
      hint: d.recipientIds.length > 1 ? 'Group DM' : 'Direct Message',
      guildId: HOME,
      channelId: d.id,
    });
  }
  for (const u of Object.values(src.users)) {
    if (u.id === src.meId) continue;
    const dm = src.dms.find((d) => d.recipientIds.length === 1 && d.recipientIds[0] === u.id);
    items.push({
      key: `user:${u.id}`,
      kind: 'user',
      label: u.displayName,
      hint: u.username,
      guildId: HOME,
      ...(dm ? { channelId: dm.id } : {}),
    });
  }
  for (const g of src.guilds) {
    items.push({
      key: `guild:${g.id}`,
      kind: 'guild',
      label: g.name,
      hint: 'Server',
      guildId: g.id,
    });
    for (const c of src.channelsByGuild[g.id] ?? []) {
      if (isTextLike(c))
        items.push({
          key: `ch:${c.id}`,
          kind: 'text',
          label: c.name,
          hint: g.name,
          guildId: g.id,
          channelId: c.id,
        });
      else if (isVoiceLike(c))
        items.push({
          key: `ch:${c.id}`,
          kind: 'voice',
          label: c.name,
          hint: g.name,
          guildId: g.id,
          channelId: c.id,
        });
    }
  }
  return items;
}

/** Higher is better; null means no match. */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 0;
  if (t === q) return 2000;
  if (t.startsWith(q)) return 1000 - t.length;
  const idx = t.indexOf(q);
  if (idx >= 0) return 500 - idx;
  let ti = 0;
  let gaps = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    gaps += found - ti;
    ti = found + 1;
  }
  return 100 - gaps;
}

const PREFIX: Record<string, SwitcherKind[]> = {
  '@': ['user', 'dm'],
  '#': ['text'],
  '!': ['voice'],
  '*': ['guild'],
};

export function parseQuery(raw: string): { kinds: SwitcherKind[] | null; text: string } {
  const first = raw.charAt(0);
  const kinds = PREFIX[first];
  return kinds ? { kinds, text: raw.slice(1).trim() } : { kinds: null, text: raw.trim() };
}

export function searchSwitcher(
  items: readonly SwitcherItem[],
  raw: string,
  recent: readonly string[],
  limit = 12,
): SwitcherItem[] {
  const { kinds, text } = parseQuery(raw);
  const pool = kinds ? items.filter((i) => kinds.includes(i.kind)) : items;
  if (!text && !kinds) {
    const byKey = new Map(items.map((i) => [i.key, i]));
    const recents = recent.map((k) => byKey.get(k)).filter((i): i is SwitcherItem => !!i);
    return (
      recents.length ? recents : items.filter((i) => i.kind === 'dm' || i.kind === 'guild')
    ).slice(0, limit);
  }
  return pool
    .map((item) => {
      const s = fuzzyScore(text, item.label);
      const h = item.kind === 'text' || item.kind === 'voice' ? fuzzyScore(text, item.hint) : null;
      // A hint match (server name) ranks below any label match.
      const score = s ?? (h !== null ? h - 1000 : null);
      return { item, score, recent: recent.indexOf(item.key) };
    })
    .filter((r): r is { item: SwitcherItem; score: number; recent: number } => r.score !== null)
    .sort(
      (a, b) =>
        b.score - a.score ||
        (a.recent < 0 ? 99 : a.recent) - (b.recent < 0 ? 99 : b.recent) ||
        a.item.label.localeCompare(b.item.label),
    )
    .slice(0, limit)
    .map((r) => r.item);
}
