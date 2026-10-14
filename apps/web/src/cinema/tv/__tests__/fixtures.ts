// MSW fixtures for the Live TV tests: a three-channel guide around a fixed "now" (Tue 10 Mar 2026, 20:15 local).
import { http, HttpResponse, server } from '../../../test/server';
import { MIN, startOfDay } from '../model';
import type { TuneGrant, TvBlock, TvChannel, TvGuide } from '../types';

export const NOW = new Date(2026, 2, 10, 20, 15, 0).getTime();
export const SLOT0 = new Date(2026, 2, 10, 20, 0, 0).getTime();

export const CHANNELS: TvChannel[] = [
  {
    id: 'c101',
    number: 101,
    call_sign: '[adult swim]',
    name: 'Adult Swim',
    network: 'Adult Swim',
    kind: 'imported_guide',
    category: 'anime',
    timezone: 'America/New_York',
    age_restricted: false,
  },
  {
    id: 'c102',
    number: 102,
    call_sign: 'CARTOON NETWORK',
    name: 'Cartoon Network',
    network: 'Cartoon Network',
    kind: 'rule_based',
    category: 'series',
    timezone: 'UTC',
    age_restricted: false,
  },
  {
    id: 'c105',
    number: 105,
    call_sign: 'MOVIES',
    name: 'Movie Channel',
    network: 'CUSTOM',
    kind: 'manual',
    category: 'movies',
    timezone: 'UTC',
    age_restricted: true,
  },
];

let seq = 0;
export const block = (
  channel: string,
  startMin: number,
  lenMin: number,
  o: Partial<TvBlock> = {},
): TvBlock => ({
  id: `b${++seq}`,
  channel_id: channel,
  start_ms: SLOT0 + startMin * MIN,
  end_ms: SLOT0 + (startMin + lenMin) * MIN,
  title: 'Programme',
  kind: 'series',
  year: 2000,
  ...o,
});

export function guide(): TvGuide {
  seq = 0;
  return {
    now_ms: NOW,
    channels: CHANNELS,
    footer_note: null,
    blocks: {
      c101: [
        block('c101', -30, 30, { title: 'Earlier show', kind: 'anime', year: 1997 }),
        block('c101', 0, 30, {
          id: 'bebop',
          title: 'Cowboy Bebop',
          kind: 'anime',
          year: 1998,
          title_id: '3',
          episode_label: 'S1 · E4 · Gateway Shuffle',
          content_rating: 'TV-14',
          synopsis: 'Spike and the crew hunt bounties across the solar system.',
          runtime_min: 30,
        }),
        block('c101', 30, 30, {
          id: 'champloo',
          title: 'Samurai Champloo',
          kind: 'anime',
          year: 2004,
        }),
        block('c101', 90, 30, {
          id: 'bebop2',
          title: 'Cowboy Bebop',
          kind: 'anime',
          title_id: '3',
        }),
      ],
      c102: [
        block('c102', 0, 60, {
          id: 'dexter',
          title: "Dexter's Laboratory",
          year: 1996,
          title_id: '7',
        }),
        block('c102', 60, 30, { id: 'batman', title: 'Batman: The Animated Series', year: 1992 }),
      ],
      c105: [
        block('c105', 0, 120, {
          id: 'alien',
          title: 'Alien',
          kind: 'movie',
          year: 1979,
          age_restricted: true,
          content_rating: 'R',
          title_id: '4',
        }),
      ],
    },
  };
}

export const grant = (o: Partial<TuneGrant> = {}): TuneGrant => ({
  channel: CHANNELS[0]!,
  master_url: 'https://cdn.test/tv/c101/master.m3u8?sig=a',
  expires_at_ms: NOW + 3600_000,
  refresh_at_ms: null,
  now: guide().blocks.c101![1]!,
  next: guide().blocks.c101![2]!,
  server_time_ms: NOW,
  dvr_window_s: null,
  ...o,
});

export interface TvCalls {
  guide: string[];
  settingsPatches: unknown[];
  tunes: string[];
  reminders: string[];
}

/** Handlers for a signed-in member and the guide. `perms` is the admin bitfield (default: not staff). */
export function useTvApi(
  o: {
    guide?: TvGuide;
    guideStatus?: number;
    favourites?: string[];
    perms?: string | null;
    tune?: (id: string) => Response | undefined;
  } = {},
) {
  const calls: TvCalls = { guide: [], settingsPatches: [], tunes: [], reminders: [] };
  let favourites = o.favourites ?? [];
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/users/@me/settings', () =>
      HttpResponse.json({ version: 1, settings: { cinema: { tv: { favourites } } } }),
    ),
    http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
      const body = (await request.json()) as {
        settings: { cinema: { tv: { favourites: string[] } } };
      };
      calls.settingsPatches.push(body);
      favourites = body.settings.cinema.tv.favourites;
      return HttpResponse.json({ version: 2, settings: body.settings });
    }),
    http.get('*/api/v1/admin/me', () =>
      o.perms
        ? HttpResponse.json({ user_id: '1', is_owner: false, platform_permissions: o.perms })
        : HttpResponse.json({ message: 'Forbidden' }, { status: 403 }),
    ),
    http.get('*/api/v1/sr/tv/guide', ({ request }) => {
      calls.guide.push(new URL(request.url).search);
      if (o.guideStatus) return new HttpResponse(null, { status: o.guideStatus });
      return HttpResponse.json(o.guide ?? guide());
    }),
    http.get('*/api/v1/sr/tv/channels/:id/now', ({ params }) => {
      const g = o.guide ?? guide();
      const ch = g.channels.find((c) => c.id === params.id);
      const list = g.blocks[String(params.id)] ?? [];
      return ch
        ? HttpResponse.json({
            channel: ch,
            now: list.find((b) => b.start_ms <= NOW && NOW < b.end_ms) ?? null,
            next: list.find((b) => b.start_ms > NOW) ?? null,
            healthy: true,
            server_time_ms: NOW,
          })
        : HttpResponse.json({ message: 'nf' }, { status: 404 });
    }),
    http.post('*/api/v1/sr/tv/channels/:id/tune', ({ params }) => {
      calls.tunes.push(String(params.id));
      const custom = o.tune?.(String(params.id));
      if (custom) return custom;
      const ch = CHANNELS.find((c) => c.id === params.id) ?? CHANNELS[0]!;
      return HttpResponse.json(grant({ channel: ch }));
    }),
    http.put('*/api/v1/sr/tv/reminders/:id', ({ params }) => {
      calls.reminders.push(String(params.id));
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return calls;
}

export { startOfDay };
