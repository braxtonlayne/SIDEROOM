// The ONE file of MSW mocks for the BYOS routes (BYOS-UI-001 against BYOS-MNT-001..003 before
// byos-core lands). When the real handlers exist, delete this file and the `server.use(...)` calls
// that import it; nothing in the screens depends on it. Shapes: ./types.ts.
import { http, HttpResponse, type HttpHandler } from 'msw';
import type { CorsResult, Mount, MountCreate, ProbeRow, Share, TestResult } from './types';

export interface ByosMockOptions {
  /** Starting mounts (user scope). */
  mounts?: Mount[];
  /** Mounts per scope are capped like `byos.max_mounts_per_user`. */
  cap?: number;
  /** Probe rows a test returns. Default: all pass. */
  probes?: ProbeRow[];
  /** What the CORS checker finds before Apply. Default `missing`. */
  cors?: CorsResult['state'];
  /** Outgoing Friend Shelf shares, by mount id. */
  shares?: Record<string, Share[]>;
  /** Object keys per mount id for the bucket browser; sizes are fake. */
  objects?: Record<string, string[]>;
  /** When true, listing and presign answer 409 CORS_NOT_READY. */
  corsNotReady?: boolean;
  /** `GET /users/@me/shelves/summary` answer (BYOS-SCP-001). */
  summary?: {
    cinema: number;
    library: number;
    music: number;
    catalog?: { cinema: number; library: number };
  };
}

export const MOCK_NOW = '2026-10-26T12:00:00.000Z';

export const REQUIRED_CORS = [
  'GET, HEAD, PUT from the server origin',
  'allowed header: content-type',
  'exposed: etag, content-range, accept-ranges, content-length',
];

export const PASSING_PROBES: ProbeRow[] = [
  { id: 'head_bucket', status: 'pass', message: 'The bucket exists and the key can reach it.' },
  { id: 'list', status: 'pass', message: 'Listing the prefix works.' },
  { id: 'put', status: 'pass', message: 'Write, read and delete of a probe object work.' },
  { id: 'multipart', status: 'pass', message: 'Multipart uploads can start and abort.' },
  { id: 'cors', status: 'pass', message: 'Bucket rules can be read.' },
  { id: 'list_buckets', status: 'pass', message: 'The key cannot see other buckets.' },
];

export function mockMount(over: Partial<Mount> = {}): Mount {
  return {
    id: '501',
    owner_kind: 'user',
    owner_id: '1',
    label: 'My Garage',
    provider: 'garage',
    endpoint: 'http://localhost:3900',
    region: 'garage',
    bucket: 'shelf',
    prefix: '',
    path_style: true,
    access_key_id: 'GKexample',
    key_hint: 'ab12',
    status: 'active',
    cors_ok_at: null,
    last_test_at: null,
    last_test: null,
    created_at: MOCK_NOW,
    ...over,
  };
}

/** Handlers for both scopes (`/users/@me/...` and `/guilds/:gid/...`), with in-memory state per call. */
export function byosMockHandlers(opts: ByosMockOptions = {}): HttpHandler[] {
  const cap = opts.cap ?? 3;
  const rows = new Map<string, Mount[]>([['user', [...(opts.mounts ?? [])]]]);
  const corsState = new Map<string, CorsResult['state']>();
  let next = 600;
  const bucketOf = (gid?: string) => {
    const key = gid ? `guild:${gid}` : 'user';
    if (!rows.has(key)) rows.set(key, []);
    return rows.get(key)!;
  };
  const find = (gid: string | undefined, id: string) => bucketOf(gid).find((m) => m.id === id);
  const cors = (m: Mount): CorsResult => {
    const state = corsState.get(m.id) ?? opts.cors ?? 'missing';
    return {
      state,
      required: REQUIRED_CORS,
      found: state === 'ok' ? REQUIRED_CORS : state === 'different' ? ['GET from *'] : [],
      manual_steps:
        state === 'manual' ? 'Open the bucket settings and paste the CORS rule by hand.' : null,
      checked_at: MOCK_NOW,
    };
  };
  const notFound = () =>
    HttpResponse.json({ message: 'Unknown mount', code: 10000 }, { status: 404 });

  const make = (prefix: string, scoped: boolean): HttpHandler[] => {
    const gidOf = (p: Record<string, unknown>) => (scoped ? String(p.gid) : undefined);
    return [
      http.get(`*/api/v1/${prefix}/storage/mounts`, ({ params }) =>
        HttpResponse.json(bucketOf(gidOf(params))),
      ),
      http.post(`*/api/v1/${prefix}/storage/mounts`, async ({ request, params }) => {
        const list = bucketOf(gidOf(params));
        if (list.length >= cap)
          return HttpResponse.json(
            { message: 'Mount limit reached', code: 'MOUNT_CAP_REACHED' },
            { status: 409 },
          );
        const body = (await request.json()) as MountCreate;
        if (list.some((x) => x.label === body.label))
          return HttpResponse.json(
            { message: 'Label taken', code: 'MOUNT_LABEL_TAKEN' },
            { status: 409 },
          );
        const m = mockMount({
          id: String(next++),
          owner_kind: scoped ? 'guild' : 'user',
          owner_id: gidOf(params) ?? '1',
          label: body.label,
          provider: body.provider,
          endpoint: body.endpoint ?? null,
          region: body.region ?? null,
          bucket: body.bucket,
          prefix: body.prefix ?? '',
          path_style: body.path_style,
          access_key_id: body.access_key_id,
          key_hint: body.access_key_id.slice(-4),
        });
        list.push(m);
        return HttpResponse.json(m, { status: 201 });
      }),
      http.patch(`*/api/v1/${prefix}/storage/mounts/:id`, async ({ request, params }) => {
        const m = find(gidOf(params), String(params.id));
        if (!m) return notFound();
        const body = (await request.json()) as Partial<Mount>;
        Object.assign(m, body);
        return HttpResponse.json(m);
      }),
      http.delete(`*/api/v1/${prefix}/storage/mounts/:id`, ({ params }) => {
        const list = bucketOf(gidOf(params));
        const i = list.findIndex((m) => m.id === params.id);
        if (i < 0) return notFound();
        list.splice(i, 1);
        return new HttpResponse(null, { status: 204 });
      }),
      http.post(`*/api/v1/${prefix}/storage/mounts/:id/test`, ({ params }) => {
        const m = find(gidOf(params), String(params.id));
        if (!m) return notFound();
        const result: TestResult = { at: MOCK_NOW, rows: opts.probes ?? PASSING_PROBES };
        m.last_test = result;
        m.last_test_at = result.at;
        m.status = result.rows.some((r) => r.status === 'fail') ? 'failing' : 'active';
        return HttpResponse.json(result);
      }),
      http.post(`*/api/v1/${prefix}/storage/mounts/:id/cors/check`, ({ params }) => {
        const m = find(gidOf(params), String(params.id));
        return m ? HttpResponse.json(cors(m)) : notFound();
      }),
      http.post(`*/api/v1/${prefix}/storage/mounts/:id/cors/apply`, ({ params }) => {
        const m = find(gidOf(params), String(params.id));
        if (!m) return notFound();
        if ((corsState.get(m.id) ?? opts.cors) === 'manual') return HttpResponse.json(cors(m));
        corsState.set(m.id, 'ok');
        m.cors_ok_at = MOCK_NOW;
        return HttpResponse.json(cors(m));
      }),
    ];
  };
  // Friend Shelf shares of the caller's own mounts (BYOS-SCP-001; the GET list is assumed).
  const shares = new Map<string, Share[]>(Object.entries(opts.shares ?? {}));
  const shareHandlers = [
    http.get('*/api/v1/users/@me/storage/mounts/:id/shares', ({ params }) =>
      HttpResponse.json(shares.get(String(params.id)) ?? []),
    ),
    http.put(
      '*/api/v1/users/@me/storage/mounts/:id/shares/:friend',
      async ({ request, params }) => {
        const flags = (await request.json()) as Omit<Share, 'friend_id'>;
        const row: Share = { friend_id: String(params.friend), ...flags };
        const list = (shares.get(String(params.id)) ?? []).filter(
          (s) => s.friend_id !== row.friend_id,
        );
        shares.set(String(params.id), [...list, row]);
        return HttpResponse.json(row);
      },
    ),
    http.delete('*/api/v1/users/@me/storage/mounts/:id/shares/:friend', ({ params }) => {
      shares.set(
        String(params.id),
        (shares.get(String(params.id)) ?? []).filter((s) => s.friend_id !== params.friend),
      );
      return new HttpResponse(null, { status: 204 });
    }),
  ];
  // Bucket browser read side (BYOS-SCP-002; assumed shapes in ../../../storage/api.ts).
  const notReady = () =>
    HttpResponse.json({ message: 'CORS not ready', code: 'CORS_NOT_READY' }, { status: 409 });
  const browseHandlers = [
    http.get('*/api/v1/storage/mounts/:id/objects', ({ request, params }) => {
      if (opts.corsNotReady) return notReady();
      const url = new URL(request.url);
      const prefix = url.searchParams.get('prefix') ?? '';
      const keys = (opts.objects ?? {})[String(params.id)] ?? [];
      const folders = new Set<string>();
      const objects: { key: string; size: number; last_modified: string | null }[] = [];
      for (const k of keys) {
        if (!k.startsWith(prefix)) continue;
        const rest = k.slice(prefix.length);
        const i = rest.indexOf('/');
        if (i >= 0) folders.add(prefix + rest.slice(0, i + 1));
        else objects.push({ key: k, size: 2048, last_modified: MOCK_NOW });
      }
      return HttpResponse.json({ folders: [...folders], objects, next_cursor: null });
    }),
    http.post('*/api/v1/storage/mounts/:id/presign', async ({ request }) => {
      if (opts.corsNotReady) return notReady();
      const body = (await request.json()) as { key: string };
      return HttpResponse.json({
        url: `https://bucket.example/${encodeURI(body.key)}?X-Amz-Signature=mock`,
        expires_at: new Date(Date.now() + 900_000).toISOString(),
      });
    }),
  ];
  const summaryHandler = http.get('*/api/v1/users/@me/shelves/summary', () =>
    HttpResponse.json(opts.summary ?? { cinema: 0, library: 0, music: 0 }),
  );
  return [
    summaryHandler,
    ...browseHandlers,
    ...shareHandlers,
    ...make('users/@me', false),
    ...make('guilds/:gid', true),
  ];
}
