import type { Channel, DmChannel, Snowflake } from './data';

export const HOME = '@me';

export const isTextLike = (c: Channel) =>
  c.type === 'text' ||
  c.type === 'announcement' ||
  c.type === 'rules' ||
  c.type === 'forum' ||
  c.type === 'media';
export const isVoiceLike = (c: Channel) => c.type === 'voice' || c.type === 'stage';

export interface ChannelGroup {
  category: Channel | null;
  channels: Channel[];
}

/** Uncategorized channels first, then each category, all by position. */
export function groupChannels(
  all: readonly Channel[],
  opts: { hideMuted?: boolean; activeId?: Snowflake } = {},
): ChannelGroup[] {
  const sorted = [...all].sort((a, b) => a.position - b.position);
  const cats = sorted.filter((c) => c.type === 'category');
  const visible = (c: Channel) =>
    !(opts.hideMuted && c.muted && c.id !== opts.activeId && !c.unread);
  const groups: ChannelGroup[] = [];
  const loose = sorted.filter(
    (c) =>
      c.type !== 'category' &&
      (!c.parentId || !cats.some((k) => k.id === c.parentId)) &&
      visible(c),
  );
  if (loose.length) groups.push({ category: null, channels: loose });
  for (const cat of cats) {
    groups.push({
      category: cat,
      channels: sorted.filter((c) => c.parentId === cat.id && visible(c)),
    });
  }
  return groups;
}

/** Channels in sidebar order that the viewer can open (for Alt+Up/Down). */
export function navigableChannels(all: readonly Channel[]): Channel[] {
  return groupChannels(all)
    .flatMap((g) => g.channels)
    .filter(isTextLike);
}

export function firstTextChannel(all: readonly Channel[]): Channel | undefined {
  return navigableChannels(all)[0];
}

/** Next/previous index with wrap-around; -1 current means "none selected". */
export function step<T>(list: readonly T[], current: number, dir: 1 | -1): T | undefined {
  if (!list.length) return undefined;
  if (current < 0) return dir === 1 ? list[0] : list[list.length - 1];
  return list[(current + dir + list.length) % list.length];
}

export function dmTitle(dm: DmChannel, nameOf: (id: Snowflake) => string): string {
  return dm.name ?? dm.recipientIds.map(nameOf).join(', ');
}
