/**
 * Active Now (LNG-FRD-013): what each friend is doing, one card per friend, richest first:
 * a joinable watch party, a screen share, a voice channel, then a game / watching / reading / listening activity.
 */
import type { Activity, PresenceStatus } from '../data';
import type { RelationshipRow } from './api';

export interface VoiceRef {
  userId: string;
  guildId: string | null;
  channelId: string;
  stream: boolean;
}

interface Base {
  row: RelationshipRow;
  status: PresenceStatus;
}

export type ActiveCard =
  | (Base & { kind: 'party'; activity: Activity; partyId: string })
  | (Base & { kind: 'stream'; where: Where })
  | (Base & { kind: 'voice'; where: Where; others: string[] })
  | (Base & { kind: 'activity'; activity: Activity });

export interface Where {
  guildId: string | null;
  channelId: string;
  guildName: string | null;
  channelName: string | null;
}

export interface ActiveInput {
  friends: readonly RelationshipRow[];
  presence: Record<
    string,
    { status: PresenceStatus; activities?: Activity[] | undefined } | undefined
  >;
  voice: readonly VoiceRef[];
  guildName(id: string): string | null;
  channelName(id: string): string | null;
}

export function buildActiveCards(input: ActiveInput): ActiveCard[] {
  const out: ActiveCard[] = [];
  const byChannel = new Map<string, VoiceRef[]>();
  for (const v of input.voice)
    byChannel.set(v.channelId, [...(byChannel.get(v.channelId) ?? []), v]);
  for (const row of input.friends) {
    const p = input.presence[row.id];
    const mine = input.voice.find((v) => v.userId === row.id);
    // A friend can be in voice while showing offline-ish; voice wins, offline without voice shows nothing.
    if (!mine && (!p || p.status === 'offline')) continue;
    const status = p?.status ?? 'online';
    const activity = p?.activities?.[0];
    const party = p?.activities?.find((a) => a.partyId);
    if (party?.partyId) {
      out.push({ kind: 'party', row, status, activity: party, partyId: party.partyId });
      continue;
    }
    if (mine) {
      const where: Where = {
        guildId: mine.guildId,
        channelId: mine.channelId,
        guildName: mine.guildId ? input.guildName(mine.guildId) : null,
        channelName: input.channelName(mine.channelId),
      };
      if (mine.stream) out.push({ kind: 'stream', row, status, where });
      else
        out.push({
          kind: 'voice',
          row,
          status,
          where,
          others: (byChannel.get(mine.channelId) ?? [])
            .map((v) => v.userId)
            .filter((id) => id !== row.id),
        });
      continue;
    }
    if (activity) out.push({ kind: 'activity', row, status, activity });
  }
  return out;
}
