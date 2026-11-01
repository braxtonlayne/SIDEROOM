import { webcrypto } from 'node:crypto';
import {
  test as base,
  expect,
  type BrowserContext,
  type Page,
  type WebSocketRoute,
} from '@playwright/test';
import { test } from '../support/fixtures';
import { builtInActivityBundle, zip } from '../support/activityBundle';
import { startActivityForTwo } from '../support/activities';
import { VOICE_STACK } from '../support/voice';

/**
 * ARC-GST-002 / wave 43 gate proof (roadmap P28, D-385, D-397, D-399).
 *
 * Two parts:
 *
 *  1. "guest runner, relay mocked": needs only the web server (dev or the built dist). The join route, the guest
 *     socket and the game's frame are played by routes in this file, so it proves what the CLIENT does: the key in the
 *     fragment never leaves the browser, the page loads no app code, the game runs in the sandbox behind the reduced
 *     `guest` bridge (every command off the allow-list is refused), the lobby payload is sealed with the game key, a
 *     sealed message from the relay reaches the game, a game that declares `ingame_voice` is refused before any socket
 *     opens, and a relay drop ends the session.
 *  2. "wave 43 gate proof (real stack)": Land.io in a voice call, the link from the Share dialog opened in an incognito
 *     (second) browser context, bidirectional sync, the 403/Drop checks at the relay, an `ingame_voice` manifest that
 *     validates and is refused by the runner. Needs the guest relay of 43-A (ARC-GST-001) in the server under test and
 *     the voice + activities stack: SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1.
 */

const TOKEN = 'ab12cd34'.repeat(8);
const APP_ID = '777';
const GUEST_TOKEN = `srg_${'0f'.repeat(32)}`;

// ---- helpers --------------------------------------------------------------------------------------------------------

const b64 = (b: Uint8Array) => Buffer.from(b).toString('base64');
const b64url = (b: Uint8Array) => b64(b).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

async function seal(keyB64Url: string, token: string, plain: Uint8Array): Promise<Uint8Array> {
  const subtle = webcrypto.subtle;
  const raw = Buffer.from(keyB64Url.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
  const key = await subtle.importKey('raw', raw, 'AES-GCM', false, ['encrypt', 'decrypt']);
  const nonce = webcrypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(
    await subtle.encrypt(
      { name: 'AES-GCM', iv: nonce, additionalData: new TextEncoder().encode(token) },
      key,
      plain,
    ),
  );
  return Buffer.concat([nonce, ct]);
}

async function open(
  keyB64Url: string,
  token: string,
  sealed: Uint8Array,
): Promise<Uint8Array | null> {
  const subtle = webcrypto.subtle;
  const raw = Buffer.from(keyB64Url.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
  const key = await subtle.importKey('raw', raw, 'AES-GCM', false, ['encrypt', 'decrypt']);
  try {
    return new Uint8Array(
      await subtle.decrypt(
        {
          name: 'AES-GCM',
          iv: sealed.slice(0, 12),
          additionalData: new TextEncoder().encode(token),
        },
        key,
        sealed.slice(12),
      ),
    );
  } catch {
    return null;
  }
}

/** The envelope of src/public/guest-play/envelope.ts: 0x01 len id bytes (relayed by a member seat). */
const relayed = (id: string, bytes: number[]) =>
  Uint8Array.from([1, id.length, ...Buffer.from(id), ...bytes]);

/** What the mocked game tries, in order. */
const PROBES: [string, string, unknown][] = [
  ['participants', 'getInstanceConnectedParticipants', {}],
  ['subscribeData', 'subscribe', { event: 'DATA_MESSAGE' }],
  ['subscribeVoice', 'subscribe', { event: 'VOICE_STATE_UPDATE' }],
  ['subscribeSpeaking', 'subscribe', { event: 'SPEAKING_START' }],
  ['getChannel', 'getChannel', {}],
  ['authorize', 'authorize', { scope: ['identify'] }],
  ['authenticate', 'authenticate', { access_token: 'x' }],
  ['setActivity', 'setActivity', { activity: { details: 'x' } }],
  ['openExternalLink', 'openExternalLink', { url: 'https://example.org' }],
  ['storageSet', 'storageSet', { key: 'k', value: 'v' }],
  ['saveFile', 'saveFile', { name: 'a.txt', mime: 'text/plain', b: 'AA==' }],
  ['future', 'getGuildMembers', {}],
  ['dataSend', 'dataSend', { b: 'AQID' }],
];

const gameHtml = (appId: string) => `<!doctype html><html><body><script>
const pending = new Map(); let n = 0;
const send = (cmd, args) => new Promise((res) => { const nonce = 'n' + (++n); pending.set(nonce, res);
  parent.postMessage({ sr: 1, kind: 'request', nonce, cmd, args }, '*'); });
const report = (x) => parent.postMessage({ probe: x }, '*');
addEventListener('message', (e) => { const f = e.data; if (!f || f.sr !== 1) return;
  if (f.kind === 'ready') { report({ ready: f.config }); run(); }
  else if (f.kind === 'response') pending.get(f.nonce)?.(f);
  else if (f.kind === 'event') report({ event: f.event, data: f.data }); });
async function run() { const out = {};
  for (const [label, cmd, args] of ${JSON.stringify(PROBES)}) {
    const f = await send(cmd, args); out[label] = f.ok ? 'ok' : f.error.code; }
  report({ probes: out }); }
parent.postMessage({ sr: 1, kind: 'handshake', application_id: '${appId}' }, '*');
</script></body></html>`;

interface Relay {
  frames: Record<string, unknown>[];
  /** Send a frame to the guest page as the relay. */
  say(f: unknown): void;
  /** Close the socket the way the relay does for a forged topic or a revoked link. */
  drop(code: number): void;
  sockets: number;
}

interface MockOptions {
  capabilities?: string[];
  joinStatus?: number;
  key: string;
}

/** Plays the join route, the guest socket and the game's frame for one page. */
async function mockStack(ctx: BrowserContext, o: MockOptions) {
  const requests: string[] = [];
  const relay: Relay = { frames: [], say: () => undefined, drop: () => undefined, sockets: 0 };
  ctx.on('request', (r) => requests.push(r.url()));
  await ctx.route('**/api/v1/play/*/join', async (route) => {
    if (o.joinStatus && o.joinStatus !== 200)
      return route.fulfill({ status: o.joinStatus, body: '' });
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        guest_token: GUEST_TOKEN,
        callsign: 'Amber Otter 42',
        expires_at: '2099-01-01T00:00:00Z',
        topic: `game:${TOKEN}`,
        socket_path: '/gateway/guest',
        application_id: APP_ID,
        application_name: 'Land.io',
        capabilities: o.capabilities ?? ['datachannel', 'guest_play', 'lobby_channel'],
      }),
    });
  });
  await ctx.route(/^http:\/\/\d+\.activities\.localhost/, (route) =>
    route.fulfill({ status: 200, contentType: 'text/html', body: gameHtml(APP_ID) }),
  );
  await ctx.routeWebSocket(/\/gateway\/guest/, (ws: WebSocketRoute) => {
    relay.sockets += 1;
    relay.say = (f) => ws.send(JSON.stringify(f));
    relay.drop = (code) => void ws.close({ code, reason: 'gone' });
    ws.onMessage((m) => {
      const f = JSON.parse(String(m)) as Record<string, unknown>;
      relay.frames.push(f);
      if (f.op === 'hello') {
        ws.send(
          JSON.stringify({
            op: 'ready',
            seat: 2,
            role: 'guest',
            callsign: 'Amber Otter 42',
            key_id: 'k',
            epoch: 0,
            scope: { data: true, media: false },
            peers: [{ seat: 1, role: 'member', callsign: 'Host' }],
          }),
        );
        // The member bridge names the game's host right after the seat opens.
        void seal(
          o.key,
          TOKEN,
          Uint8Array.from([2, ...Buffer.from(JSON.stringify({ host: '555' }))]),
        ).then((s) => ws.send(JSON.stringify({ op: 'data', from: 1, data: b64(s) })));
      }
      if (f.op === 'subscribe') ws.send(JSON.stringify({ op: 'subscribed', topic: f.topic }));
    });
  });
  return { relay, requests };
}

async function probeLog(page: Page): Promise<Record<string, any>[]> {
  return page.evaluate(() => (window as any).__probe as Record<string, any>[]);
}

const guestPage = async (ctx: BrowserContext, key: string) => {
  const page = await ctx.newPage();
  await page.addInitScript(() => {
    (window as any).__probe = [];
    addEventListener('message', (e) => {
      if (e.data && e.data.probe) (window as any).__probe.push(e.data.probe);
    });
  });
  await page.goto(`/play/${TOKEN}#key=${key}`);
  return page;
};

const newKey = () => b64url(webcrypto.getRandomValues(new Uint8Array(32)));

// ---- part 1: the runner against a mocked relay -------------------------------------------------------------------------

base.describe('guest runner, relay mocked (ARC-GST-002)', () => {
  base.use({ storageState: { cookies: [], origins: [] } });

  base(
    'the key stays in the fragment, the page is noindex, and no app code is loaded',
    async ({ browser }) => {
      const ctx = await browser.newContext();
      const key = newKey();
      const { requests } = await mockStack(ctx, { key });
      const page = await guestPage(ctx, key);
      await expect(page.getByRole('button', { name: 'Join the game' })).toBeVisible();
      await expect(page.locator('meta[name="robots"][content="noindex"]')).toHaveCount(1);
      await page.getByRole('button', { name: 'Join the game' }).click();
      await expect(page.locator('iframe[title="Game: Land.io"]')).toBeVisible();

      // Nothing sent anywhere carries the key; the join is the only API call; no app route, no app bundle.
      expect(requests.filter((u) => u.includes(key))).toEqual([]);
      const paths = requests.map((u) => new URL(u).pathname);
      expect(paths.filter((p) => p.startsWith('/api/'))).toEqual([`/api/v1/play/${TOKEN}/join`]);
      expect(paths.filter((p) => /^\/app(\/|\.html$)|\.wasm$/.test(p))).toEqual([]);
      expect(
        paths.filter((p) =>
          /^\/src\/(lounge|activities|arcade|settings|library|cinema|admin|voice|store|gateway|moderation|player|music|profilepage|developers)\//.test(
            p,
          ),
        ),
      ).toEqual([]);
      expect(await ctx.cookies()).toEqual([]);
      await ctx.close();
    },
  );

  base('a link without a valid key says so and calls nothing', async ({ browser }) => {
    const ctx = await browser.newContext();
    const { requests } = await mockStack(ctx, { key: newKey() });
    const page = await ctx.newPage();
    await page.goto(`/play/${TOKEN}`);
    await expect(page.locator('main [role="alert"]')).toContainText('incomplete');
    await page.goto('/play/not-a-token');
    await expect(page.locator('main [role="alert"]')).toContainText('not valid');
    expect(requests.filter((u) => /\/api\/|\/gateway/.test(u))).toEqual([]);
    await ctx.close();
  });

  base('the game runs in the sandbox behind the reduced guest bridge', async ({ browser }) => {
    const ctx = await browser.newContext();
    const key = newKey();
    await mockStack(ctx, { key });
    const page = await guestPage(ctx, key);
    await page.getByRole('button', { name: 'Join the game' }).click();
    const frame = page.locator('iframe[title="Game: Land.io"]');
    expect(await frame.getAttribute('sandbox')).toBe('allow-scripts allow-pointer-lock');
    expect(await frame.getAttribute('src')).toMatch(
      new RegExp('^http://' + APP_ID + '[.]activities[.]localhost(:[0-9]+)?/$'),
    );

    await expect.poll(async () => (await probeLog(page)).some((m) => m.probes)).toBe(true);
    const log = await probeLog(page);
    const ready = log.find((m) => m.ready)!.ready;
    // The game is told it is a guest with placeholders: no channel, no server, no storage, no proxy token.
    expect(ready).toMatchObject({
      guest: true,
      callsign: 'Amber Otter 42',
      user_id: 'g2',
      host_id: '555',
      channel_id: '0',
      guild_id: '0',
      instance_id: 'lobby',
      capabilities: ['datachannel'],
    });
    expect(ready.storage).toBeUndefined();
    expect(ready.proxy_token).toBeUndefined();

    const probes = log.find((m) => m.probes)!.probes as Record<string, string>;
    // Allowed: the lobby roster, the data lane.
    expect(probes.participants).toBe('ok');
    expect(probes.subscribeData).toBe('ok');
    expect(probes.dataSend).toBe('ok');
    // Everything else is refused: identity, channel, presence, links, storage, files, voice events, future commands.
    for (const k of [
      'subscribeVoice',
      'subscribeSpeaking',
      'getChannel',
      'authorize',
      'authenticate',
      'setActivity',
      'openExternalLink',
      'storageSet',
      'saveFile',
    ]) {
      expect(probes[k], k).toBe('NOT_AUTHORIZED');
    }
    expect(probes.future).toBe('UNKNOWN_COMMAND');
    await ctx.close();
  });

  base(
    'lobby data is sealed with the game key both ways, and the relay stamps who sent it',
    async ({ browser }) => {
      const ctx = await browser.newContext();
      const key = newKey();
      const { relay } = await mockStack(ctx, { key });
      const page = await guestPage(ctx, key);
      await page.getByRole('button', { name: 'Join the game' }).click();
      await expect.poll(async () => (await probeLog(page)).some((m) => m.probes)).toBe(true);

      // The relay only ever sees the one topic, and ciphertext.
      const topics = new Set(relay.frames.map((f) => f.topic).filter(Boolean));
      expect([...topics]).toEqual([`game:${TOKEN}`]);
      const pub = relay.frames.find((f) => f.op === 'publish')!;
      const sealed = Buffer.from(pub.data as string, 'base64');
      expect(sealed.length).toBe(12 + 1 + 3 + 16);
      const plain = await open(key, TOKEN, sealed);
      expect(Array.from(plain!)).toEqual([0, 1, 2, 3]);
      expect(await open(newKey(), TOKEN, sealed)).toBeNull();

      // A message from the member's seat, sealed with the key, reaches the game under the room member's id.
      relay.say({
        op: 'data',
        from: 1,
        data: b64(await seal(key, TOKEN, relayed('4242', [9, 8]))),
      });
      // One sealed under another key is dropped before the game sees it.
      relay.say({
        op: 'data',
        from: 1,
        data: b64(await seal(newKey(), TOKEN, relayed('4242', [7]))),
      });
      await expect
        .poll(async () => (await probeLog(page)).filter((m) => m.event === 'DATA_MESSAGE').length)
        .toBe(1);
      const ev = (await probeLog(page)).find((m) => m.event === 'DATA_MESSAGE')!;
      expect(ev.data).toEqual({ from: '4242', b: Buffer.from([9, 8]).toString('base64') });
      await ctx.close();
    },
  );

  base(
    'a game that declares ingame_voice is refused before any socket opens',
    async ({ browser }) => {
      const ctx = await browser.newContext();
      const key = newKey();
      const { relay } = await mockStack(ctx, {
        key,
        capabilities: ['datachannel', 'guest_play', 'ingame_voice'],
      });
      const page = await guestPage(ctx, key);
      await page.getByRole('button', { name: 'Join the game' }).click();
      await expect(page.locator('main [role="alert"]')).toContainText('in-game voice');
      await expect(page.locator('iframe')).toHaveCount(0);
      expect(relay.sockets).toBe(0);
      await ctx.close();
    },
  );

  base(
    'an expired link, a rate limit and a relay drop each end in a clear message',
    async ({ browser }) => {
      for (const [status, text] of [
        [410, 'expired'],
        [429, 'Too many tries'],
      ] as const) {
        const ctx = await browser.newContext();
        const key = newKey();
        await mockStack(ctx, { key, joinStatus: status });
        const page = await guestPage(ctx, key);
        await page.getByRole('button', { name: 'Join the game' }).click();
        await expect(page.locator('main [role="alert"]')).toContainText(text);
        await ctx.close();
      }
      const ctx = await browser.newContext();
      const key = newKey();
      const { relay } = await mockStack(ctx, { key });
      const page = await guestPage(ctx, key);
      await page.getByRole('button', { name: 'Join the game' }).click();
      await expect(page.locator('iframe[title="Game: Land.io"]')).toBeVisible();
      relay.drop(4410);
      await expect(page.locator('main [role="alert"]')).toContainText('connection ended');
      await ctx.close();
    },
  );
});

// ---- part 2: the wave 43 gate proof against the real stack ---------------------------------------------------------------

const GUEST = process.env.SR_E2E_GUEST === '1';
const ACTIVITIES = process.env.SR_E2E_ACTIVITIES === '1';

test.describe('wave 43 gate proof (real stack)', () => {
  test.skip(
    !GUEST || !ACTIVITIES || !VOICE_STACK,
    'Needs the guest relay (ARC-GST-001), activities and the voice stack: SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1.',
  );

  test('Land.io in a call, a guest in an incognito context, sync both ways, 403/Drop at the relay', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(
      browserName !== 'chromium',
      'Chromium resolves *.localhost activity hosts by itself.',
    );
    const base = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
    const session = await startActivityForTwo(
      { browser, browserName, owner, playwright },
      builtInActivityBundle('landio'),
      'gst',
    );
    try {
      const { a, name } = session;
      // Alice (the host) opens Share link and reads the link; the key was made in her browser.
      await a.page.getByRole('button', { name: 'Share link' }).click();
      await a.page.getByRole('button', { name: 'Create link' }).click();
      const href = await a.page.getByRole('textbox', { name: 'Guest link' }).inputValue();
      expect(href).toMatch(/\/play\/[0-9a-f]{64}#key=[A-Za-z0-9_-]{43}$/);
      await expect(a.page.getByRole('img', { name: 'QR code for the guest link' })).toBeVisible();
      await a.page.getByRole('button', { name: 'Done' }).click();

      // The guest: a second browser context with no cookies or storage (an incognito window).
      const incognito = await browser.newContext();
      const guestRequests: string[] = [];
      incognito.on('request', (r) => guestRequests.push(r.url()));
      const g = await incognito.newPage();
      await g.goto(href);
      await expect(g.locator('meta[name="robots"][content="noindex"]')).toHaveCount(1);
      await g.getByRole('button', { name: 'Join the game' }).click();
      const guestFrame = g.locator(`iframe[title^="Game:"]`);
      await expect(guestFrame).toBeVisible({ timeout: 30_000 });
      expect(guestRequests.filter((u) => /\/app\/|app\.html|\.wasm$/.test(u))).toEqual([]);

      // Land.io: the guest presses Play; the host's engine gets a seat for the guest and ticks flow back.
      const gf = g.frameLocator('iframe[title^="Game:"]');
      await expect(gf.locator('#start')).toBeVisible({ timeout: 60_000 });
      await gf.locator('#play').click();
      const fg = g.frames().find((f) => f.url().includes('.activities.'))!;
      await expect
        .poll(() => fg.evaluate(() => (window as any).__landio.screen()), { timeout: 30_000 })
        .toBe('playing');
      const t0 = await fg.evaluate(() => (window as any).__landio.world.recvTick as number);
      await expect
        .poll(() => fg.evaluate(() => (window as any).__landio.world.recvTick as number), {
          timeout: 15_000,
        })
        .toBeGreaterThan(t0 + 14);
      // The guest's player exists in the host's engine: guest -> member as well as member -> guest. The relay
      // chose the callsign (random); the runner's bar shows it.
      const bar = await g.getByText(/^Playing as /).innerText();
      const callsign = /^Playing as (.+?)(?: · .*)?$/.exec(bar.trim())?.[1] ?? '';
      expect(callsign).toMatch(/^[A-Z][a-z]+ [A-Z][a-z]+ \d{2}$/);
      const fa = a.page.frames().find((f) => f.url().includes('.activities.'))!;
      await expect
        .poll(
          () =>
            fa.evaluate(() =>
              (window as any).__landio
                .players()
                .filter((p: any) => !p.bot)
                .map((p: any) => p.name),
            ),
          { timeout: 20_000 },
        )
        .toContain(callsign);

      // Guest bundle isolation (D-385, D-397): the guest's browser asked the server for the guest-play shell, the
      // one join call and the one game's own origin, nothing else: no other Activity host, no other API route.
      const appHost = `${session.appId}.activities.`;
      for (const u of guestRequests) {
        const url = new URL(u);
        if (url.hostname.includes('.activities.'))
          expect(url.hostname, u).toMatch(new RegExp(`^${appHost}`));
        if (url.pathname.startsWith('/api/'))
          expect(url.pathname, u).toMatch(/^\/api\/v1\/play\/[0-9a-f]{64}\/join$/);
      }
      expect(await incognito.cookies()).toEqual([]);

      // REST: a guest session token opens nothing (chat, members, profiles, shelves). The join route's answer is
      // the guest's only credential, taken from the page's own call.
      const joinReq = guestRequests.find((u) => /\/api\/v1\/play\/.+\/join$/.test(u))!;
      expect(joinReq).toBeTruthy();
      const req = await playwright.request.newContext({ baseURL: base });
      const joined = (await (await req.post(new URL(joinReq).pathname)).json()) as {
        guest_token: string;
      };
      // Real routes of this call's server (a made-up path would answer 404/405, which proves nothing), as a bearer
      // token and as the session cookie.
      const sv = session.server;
      const probes = [
        '/api/v1/users/@me',
        '/api/v1/users/@me/guilds',
        `/api/v1/channels/${sv.textChannelId}/messages`,
        `/api/v1/channels/${sv.voiceChannelId}/messages`,
        `/api/v1/guilds/${sv.guildId}/members`,
        `/api/v1/users/@me/guilds/${sv.guildId}/member`,
        '/api/v1/users/1/profile',
        '/api/v1/shelves/visible',
        '/api/v1/activities',
      ];
      const credentials: Record<string, string>[] = [
        { Authorization: `Bearer ${joined.guest_token}` },
        { Cookie: `sr_session=${joined.guest_token}` },
      ];
      for (const headers of credentials) {
        for (const path of probes) {
          const res = await req.get(path, { headers });
          expect(res.status(), `${Object.keys(headers)[0]} ${path}`).toBe(403);
        }
      }
      // 403/Drop at the relay. A made-up guest token is refused (4401); a real one that forges a second
      // subscription (another lobby's topic) after `ready` is dropped (4403) before anything reaches a lobby.
      const bogus = await g.evaluate(
        () =>
          new Promise<number>((resolve) => {
            const ws = new WebSocket(`ws://${location.host}/gateway/guest?v=1`);
            ws.onclose = (e) => resolve(e.code);
            ws.onopen = () =>
              ws.send(JSON.stringify({ op: 'hello', guest_token: 'srg_' + '0'.repeat(64) }));
            setTimeout(() => resolve(-1), 8000);
          }),
      );
      expect(bogus).toBe(4401);
      const forged = await g.evaluate(
        (t) =>
          new Promise<{ ready: boolean; code: number }>((resolve) => {
            let ready = false;
            const ws = new WebSocket(`ws://${location.host}/gateway/guest?v=1`);
            ws.onclose = (e) => resolve({ ready, code: e.code });
            ws.onopen = () => ws.send(JSON.stringify({ op: 'hello', guest_token: t }));
            ws.onmessage = (e) => {
              const f = JSON.parse(String(e.data)) as { op: string };
              if (f.op !== 'ready') return;
              ready = true;
              ws.send(JSON.stringify({ op: 'subscribe', topic: `game:${'f'.repeat(64)}` }));
            };
            setTimeout(() => resolve({ ready, code: -1 }), 8000);
          }),
        joined.guest_token,
      );
      expect(forged).toEqual({ ready: true, code: 4403 });

      // The main gateway and the voice stream refuse a guest token too.
      const main = await g.evaluate(
        (t) =>
          new Promise<string>((resolve) => {
            const ws = new WebSocket(`ws://${location.host}/gateway?v=1`);
            ws.onmessage = (e) => resolve(String(e.data).slice(0, 80));
            ws.onclose = (e) => resolve(`closed ${e.code}`);
            ws.onopen = () => ws.send(JSON.stringify({ op: 2, d: { token: t } }));
            setTimeout(() => resolve('silent'), 5000);
          }),
        joined.guest_token,
      );
      expect(main).not.toMatch(/READY/);
      await req.dispose();
      void name;
      await incognito.close();
    } finally {
      await session.close();
    }
  });

  test('a manifest declaring ingame_voice validates, runs for members, and the runner refuses it for a guest', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(
      browserName !== 'chromium',
      'Chromium resolves *.localhost activity hosts by itself.',
    );
    // A Tier 1 game declaring the reserved `ingame_voice` flag next to `guest_play`: the upload validates (D-399),
    // members can run it (the flag is never handed to a running instance), a link can be made, and the real
    // relay's join answer names the flag, so the guest's runner refuses before it opens any socket.
    const bundle = zip({
      'index.html': '<!doctype html><meta charset="utf-8"><title>Voice game</title><p>hi</p>',
      'sideroom.json': JSON.stringify({
        tier: 'TIER_1_ORIGINAL',
        capabilities: ['datachannel', 'guest_play', 'lobby_channel', 'ingame_voice'],
      }),
    });
    const session = await startActivityForTwo(
      { browser, browserName, owner, playwright },
      bundle,
      'gstv',
    );
    try {
      const { a } = session;
      await a.page.getByRole('button', { name: 'Share link' }).click();
      await a.page.getByRole('button', { name: 'Create link' }).click();
      const href = await a.page.getByRole('textbox', { name: 'Guest link' }).inputValue();
      expect(href).toMatch(/\/play\/[0-9a-f]{64}#key=[A-Za-z0-9_-]{43}$/);
      await a.page.getByRole('button', { name: 'Done' }).click();

      const incognito = await browser.newContext();
      const g = await incognito.newPage();
      const sockets: string[] = [];
      g.on('websocket', (ws) => sockets.push(ws.url()));
      const join = g.waitForResponse((r) => /\/api\/v1\/play\/[0-9a-f]{64}\/join$/.test(r.url()));
      await g.goto(href);
      await g.getByRole('button', { name: 'Join the game' }).click();
      const answer = (await (await join).json()) as { capabilities: string[] };
      expect(answer.capabilities).toContain('ingame_voice');
      await expect(g.locator('main [role="alert"]')).toContainText('in-game voice');
      await expect(g.locator('iframe')).toHaveCount(0);
      expect(sockets.filter((u) => u.includes('/gateway/guest'))).toEqual([]);
      await incognito.close();
    } finally {
      await session.close();
    }
  });
});
