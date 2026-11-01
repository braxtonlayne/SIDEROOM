import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { APIRequestContext } from '@playwright/test';
import { apiPost, expect, test } from '../support/fixtures';
import { PUBLIC_URL } from '../support/cli';
import { findLeaks } from '../../scripts/check-crash-card.mjs';

/**
 * OPS-CRASH-001 / wave 45 gate proof, half A (roadmap P28, D-385, threat model T45).
 *
 * "Inject a synthetic WASM panic in a Playwright test, verify the allow-listed crash payload arrives in #crash-triage
 * with zero sensitive tokens."
 *
 * A signed-in member's page throws a WebAssembly RuntimeError (what a Rust panic is without a panic hook) whose
 * message, stack and ring buffer are stuffed with secrets (e2e/fixtures/crash-hostile.json). The page's own hooks build
 * the report; the test then also sends the raw hostile payload straight to the endpoint, because the server must not
 * trust the client to have stripped anything. The Owner reads #crash-triage of the Staff Server through the ordinary
 * message API: the cards are there, deduplicated, structured, and neither the fixture's secret strings nor any pattern
 * of scripts/crash-secret-patterns.json (plus the repo-wide secret-scan list) appears in them. Nothing is accepted
 * before sign-in, the body is capped, the endpoint is rate limited, and a member cannot read the triage store.
 *
 * Tagged @gate: it runs in the "Exit gates" step of the playwright job, on a server of its own (it registers two
 * accounts).
 */

const fixture = JSON.parse(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), '../fixtures/crash-hostile.json'),
    'utf8',
  ),
) as {
  payload: Record<string, unknown>;
  needles: string[];
  expect: { frames: { file: string; line: number; fn?: string }[] };
};

interface Message {
  id: string;
  content: string;
  embeds: { title?: string; description?: string }[];
  components?: unknown;
}

async function triageMessages(staff: APIRequestContext): Promise<Message[]> {
  const guilds = (await (await staff.get('/api/v1/users/@me/guilds')).json()) as {
    id: string;
    is_staff_guild?: boolean;
  }[];
  const staffGuild = guilds.find((g) => g.is_staff_guild);
  expect(staffGuild, 'the Owner is in the Staff Server').toBeTruthy();
  const channels = (await (
    await staff.get(`/api/v1/guilds/${staffGuild!.id}/channels`)
  ).json()) as {
    id: string;
    name: string;
  }[];
  const triage = channels.find((c) => c.name === 'crash-triage');
  if (!triage) return [];
  const res = await staff.get(`/api/v1/channels/${triage.id}/messages?limit=50`);
  expect(res.status()).toBe(200);
  return (await res.json()) as Message[];
}

/** The JSON of a card: the code block in the embed description. */
function cardOf(m: Message): Record<string, unknown> {
  const d = m.embeds[0]?.description ?? '';
  const text = d.replace(/^```json\n/, '').replace(/\n```$/, '');
  return JSON.parse(text) as Record<string, unknown>;
}

test.describe('crash telemetry @gate', () => {
  test('a synthetic WASM panic reaches #crash-triage allow-listed, deduplicated and clean', async ({
    browserName,
    owner,
    playwright,
    seedUser,
    signedIn,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only: the synthetic panic is a V8 error.');

    // A fresh commit and function name per run, so a rerun against the same database is a new crash, not a repeat.
    const run = randomBytes(6).toString('hex');
    const tag = Array.from({ length: 8 }, () =>
      String.fromCharCode(97 + Math.floor(Math.random() * 26)),
    ).join('');
    const hostile = { ...fixture.payload, commit: run };

    // ---- nothing before sign-in -----------------------------------------------------------------------------------
    const anon = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    const refused = await anon.post('/api/v1/crash-reports', {
      headers: { Origin: new URL(PUBLIC_URL).origin },
      data: hostile,
    });
    expect(refused.status(), 'anonymous report').toBe(401);
    await anon.dispose();

    const member = await signedIn(await seedUser('crash'));
    const staff = await signedIn(owner);
    // The Owner is the reader of the triage channel.
    const before = (await triageMessages(staff.context.request)).length;

    // ---- the panic, in the member's browser ---------------------------------------------------------------------------
    const { page } = member;
    page.on('pageerror', () => undefined); // the injected panic is unhandled on purpose
    await page.goto('/channels/@me');
    await expect(page).toHaveURL(/\/channels/);
    await page.waitForLoadState('networkidle');

    // Ticks from an in-page runtime: two enums and a hostile string that must be recorded as `other`.
    await page.evaluate(() => {
      for (const input of ['jump', 'left', 'typed: hunter2-message-body-DO-NOT-LEAK']) {
        window.dispatchEvent(new CustomEvent('sr-crash-tick', { detail: { src: 'game', input } }));
      }
    });

    const request = page.waitForRequest(
      (r) => r.method() === 'POST' && r.url().endsWith('/api/v1/crash-reports'),
    );
    const response = page.waitForResponse(
      (r) => r.request().method() === 'POST' && r.url().endsWith('/api/v1/crash-reports'),
    );
    await page.evaluate(
      (secrets) => {
        const err = new WebAssembly.RuntimeError(`unreachable: ${secrets.message}`);
        err.stack = [
          `RuntimeError: unreachable: ${secrets.message}`,
          '    at sr_e2ee_wasm::mls::decrypt (wasm://wasm/0a1b2c3d4e5f6071:wasm-function[412]:0x3f4a)',
          `    at renderChannel${secrets.tag} (https://app.example.test/assets/index-Bx3k9Qa1.js?${secrets.query}:120:15)`,
          `    at https://203.0.113.77/u/123456789012345678/avatar.js?token=abc:3:9`,
        ].join('\n');
        setTimeout(() => {
          throw err;
        }, 0);
      },
      {
        message:
          'hunter2-message-body-DO-NOT-LEAK Bearer srg_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        query: 'X-Amz-Signature=deadbeefcafe1234deadbeefcafe1234',
        tag,
      },
    );

    // What the page itself sends is already stripped.
    const sentBody = (await request).postData() ?? '';
    for (const needle of fixture.needles)
      expect(sentBody, `page body shows ${needle}`).not.toContain(needle);
    expect(findLeaks(sentBody)).toEqual([]);
    const sent = JSON.parse(sentBody) as {
      kind: string;
      route: string;
      frames: { file: string; fn?: string }[];
      ticks: { src: string; input: string }[];
    };
    expect(sent.kind).toBe('wasm_panic');
    expect(sent.route).toMatch(/^[A-Za-z0-9.:_-]+$/);
    expect(sent.frames[0]).toMatchObject({ file: 'wasm', fn: 'sr_e2ee_wasm::mls::decrypt' });
    expect(sent.frames[1]).toMatchObject({
      file: 'assets/index-Bx3k9Qa1.js',
      fn: `renderChannel${tag}`,
    });
    expect(sent.ticks.map((t) => t.input)).toEqual(['jump', 'left', 'other']);
    expect((await response).status()).toBe(202);

    // ---- the raw hostile payload, straight to the server --------------------------------------------------------------
    const first = await apiPost<{ id: string; count: number; deduplicated: boolean }>(
      member.context.request,
      '/crash-reports',
      hostile,
    );
    expect(first.deduplicated).toBe(false);
    // The same crash again is the same row with a higher count.
    const again = await apiPost<{ id: string; count: number; deduplicated: boolean }>(
      member.context.request,
      '/crash-reports',
      hostile,
    );
    expect(again).toMatchObject({ id: first.id, count: 2, deduplicated: true });

    // ---- #crash-triage ---------------------------------------------------------------------------------------------------
    const messages = await triageMessages(staff.context.request);
    expect(messages, 'one card per distinct crash').toHaveLength(before + 2);
    const everything = JSON.stringify(messages);
    for (const needle of fixture.needles) {
      expect(everything, `#crash-triage shows ${needle}`).not.toContain(needle);
    }
    expect(findLeaks(everything)).toEqual([]);

    const cards = messages.map((m) => ({ title: m.embeds[0]?.title ?? '', card: cardOf(m) }));
    const hostileCard = cards.find((c) => c.card['commit'] === run)!;
    expect(hostileCard.card).toMatchObject({
      kind: 'wasm_panic',
      route: 'unknown',
      engine: 'chromium',
      replay: ['jump', 'other', 'bridge_data_send'],
    });
    expect((hostileCard.card['frames'] as { file: string }[]).map((f) => f.file)).toEqual(
      fixture.expect.frames.map((f) => f.file),
    );
    expect(hostileCard.card['count'], 'the card is refreshed in place at the second report').toBe(
      2,
    );
    expect(hostileCard.title).toMatch(/^Crash x2: wasm_panic on unknown/);
    const browserCard = cards.find((c) =>
      (c.card['frames'] as { fn?: string }[]).some((f) => f.fn === `renderChannel${tag}`),
    )!;
    expect(browserCard.card).toMatchObject({
      kind: 'wasm_panic',
      replay: ['jump', 'left', 'other'],
    });

    // ---- an agent (a staff session) reads the replay buffer; a member cannot -----------------------------------------------
    const list = (await (
      await staff.context.request.get('/api/v1/admin/crash-reports')
    ).json()) as {
      id: string;
      count: number;
    }[];
    expect(list.map((r) => r.id)).toContain(first.id);
    const detail = (await (
      await staff.context.request.get(`/api/v1/admin/crash-reports/${first.id}`)
    ).json()) as { count: number; replay: { input: string }[] };
    expect(detail.count).toBe(2);
    expect(detail.replay.map((t) => t.input)).toEqual(['jump', 'other', 'bridge_data_send']);
    expect(findLeaks(JSON.stringify(detail))).toEqual([]);
    for (const path of ['/admin/crash-reports', `/admin/crash-reports/${first.id}`]) {
      const res = await member.context.request.get(`/api/v1${path}`);
      expect([401, 403], `member GET ${path}`).toContain(res.status());
    }
  });

  test('the endpoint caps the body and rate limits an account', async ({
    browserName,
    seedUser,
    signedIn,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only: one browser proves the server limits.');
    const member = await signedIn(await seedUser('crashlim'));
    const r = member.context.request;

    // Too big: refused before it is read (32 KiB cap).
    const big = { commit: 'abcdef1', kind: 'error', frames: [], pad: 'x'.repeat(40 * 1024) };
    await expect(apiPost(r, '/crash-reports', big)).rejects.toThrow(/-> 413/);

    // A report without a valid commit and kind is a 400, nothing is stored.
    await expect(apiPost(r, '/crash-reports', { kind: 'error' })).rejects.toThrow(/-> 400/);

    // Six an account in ten minutes; the seventh is refused.
    const report = (n: number) => ({
      commit: 'abcdef1',
      kind: 'error',
      route: 'lounge',
      frames: [{ file: `https://app.test/assets/f${n}.js`, line: n, col: 1, fn: `fn${n}` }],
    });
    // The oversize body never reached the limiter; the invalid report did count. Count from the server's own
    // answers instead of assuming where the line falls.
    let accepted = 0;
    let limited = false;
    for (let n = 1; n <= 8 && !limited; n++) {
      try {
        await apiPost(r, '/crash-reports', report(n));
        accepted += 1;
      } catch (e) {
        expect(String(e)).toMatch(/-> 429/);
        limited = true;
      }
    }
    expect(limited, 'the account is rate limited').toBe(true);
    expect(accepted).toBeGreaterThanOrEqual(1);
    expect(accepted).toBeLessThanOrEqual(6);
    // Someone else is not limited by it.
    const other = await signedIn(await seedUser('crashlim2'));
    await apiPost(other.context.request, '/crash-reports', report(99));
  });
});
