import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import type { AppVersion, BotUser, DevApp } from '../types';

export { http, HttpResponse, server };

export const BOT: BotUser = {
  id: '501',
  username: 'Jukebox',
  global_name: 'Jukebox',
  avatar: null,
  bot: true,
};

export const APP: DevApp = {
  id: '500',
  name: 'Jukebox',
  description: 'Plays songs',
  icon: null,
  tags: [],
  bot_public: false,
  bot_require_code_grant: false,
  flags: 0,
  release_state: 'development',
  owner: { id: '1', username: 'alice', global_name: 'Alice' },
  bot: BOT,
  redirect_uris: ['https://example.com/cb'],
  install_params: { scopes: ['bot', 'applications.commands'], permissions: '0' },
  terms_of_service_url: null,
  privacy_policy_url: null,
  privileged_intents: { requested: [], approved: [] },
  client_secret_hint: '...abcd',
  created_at: '2026-10-01T00:00:00Z',
};

export const VERSION: AppVersion = {
  id: '700',
  application_id: '500',
  version: 1,
  state: 'pending',
  needs_review: true,
  needs_review_reasons: ['first_release', 'scopes'],
  diff: { changed: [{ key: 'scopes', old: [], new: ['bot'] }] },
  note: 'Please review',
  reviewer_note: null,
  created_at: '2026-10-02T00:00:00Z',
  decided_at: null,
};

export function signedIn() {
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
}

export function openAt(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}

export async function readJson(request: Request): Promise<unknown> {
  try {
    return await request.clone().json();
  } catch {
    return undefined;
  }
}

/** Serve one application at GET/PATCH /applications/:id and record the PATCH bodies. */
export function serveApp(initial: DevApp = APP) {
  let current = initial;
  const patches: unknown[] = [];
  server.use(
    http.get('*/api/v1/applications/:id', () => HttpResponse.json(current)),
    http.patch('*/api/v1/applications/:id', async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      patches.push(body);
      const { privileged_intents, ...rest } = body as { privileged_intents?: string[] };
      current = { ...current, ...(rest as Partial<DevApp>) };
      if (privileged_intents)
        current = {
          ...current,
          privileged_intents: {
            ...current.privileged_intents,
            requested: privileged_intents as DevApp['privileged_intents']['requested'],
          },
        };
      return HttpResponse.json(current);
    }),
  );
  return { patches, set: (a: DevApp) => (current = a) };
}
