import { expect, type Frame, type Page } from '@playwright/test';
import { apiPost, test } from '../support/fixtures';
import { serveTone } from '../support/playerAudio';

/**
 * PRF-UNF-001: Watch Together over a direct media file and over a provider embed, two members in two browser
 * contexts. Nothing leaves the machine and nothing needs YouTube:
 *
 * - the direct file is the generated tone of the player specs (`serveTone` answers any `/e2e-tone-*.wav` request,
 *   whatever host it was asked on), addressed as `https://media.sideroom-e2e.test/...` so the server's own rules
 *   (https only, a public-looking name) apply to the party input;
 * - the "provider" is a stub page fulfilled for `https://www.youtube-nocookie.com/**` that answers the documented
 *   postMessage commands (`listening`, `playVideo`, `pauseVideo`, `seekTo`) and reports `infoDelivery` like the
 *   real iframe, so the app's real frame driver, origin checks and gate run unchanged.
 *
 * The spec asserts the two promises of the feature: nothing third-party loads before a click, and after the click
 * both members' players follow the one server-side playback state. A fixed port pair (8082/5175) is used by this
 * wave's agent (SR_E2E_BASE_URL / SR_SERVER__PUBLIC_URL as in docs/dev/e2e).
 */

const TONE_URL = 'https://media.sideroom-e2e.test/e2e-tone-watch.wav';
const YT_URL = 'https://youtu.be/dQw4w9WgXcQ';
const YT_ORIGIN = 'https://www.youtube-nocookie.com';

/** The stub provider page: the YouTube iframe protocol in 25 lines. */
const FAKE_YOUTUBE = `<!doctype html><meta charset="utf-8"><title>stub player</title><body style="margin:0;background:#123">
<script>
  let pos = 0, state = 2, at = Date.now(), timer = 0;
  const cur = () => (state === 1 ? pos + (Date.now() - at) / 1000 : pos);
  const report = () =>
    parent.postMessage(JSON.stringify({ event: 'infoDelivery',
      info: { currentTime: cur(), playerState: state, playbackRate: 1, duration: 600 } }), '*');
  window.__fake = { cur, state: () => state, commands: [] };
  addEventListener('message', (e) => {
    let m; try { m = JSON.parse(e.data); } catch { return; }
    if (m.event === 'listening') {
      parent.postMessage(JSON.stringify({ event: 'onReady' }), '*');
      report();
      if (!timer) timer = setInterval(report, 250);
    } else if (m.event === 'command') {
      window.__fake.commands.push(m.func);
      pos = cur(); at = Date.now();
      if (m.func === 'playVideo') state = 1;
      if (m.func === 'pauseVideo') state = 2;
      if (m.func === 'seekTo') pos = m.args[0];
      report();
    }
  });
</script></body>`;

async function stubProvider(page: Page, hits: string[]): Promise<void> {
  await page.route(`${YT_ORIGIN}/**`, async (route) => {
    hits.push(route.request().url());
    await route.fulfill({ status: 200, contentType: 'text/html', body: FAKE_YOUTUBE });
  });
}

const frameOf = (page: Page): Frame | undefined =>
  page.frames().find((f) => f.url().startsWith(YT_ORIGIN));

const mediaTime = (page: Page) =>
  page.evaluate(() => document.querySelector<HTMLMediaElement>('.wp-ext video')?.currentTime ?? -1);

test.describe('Watch Together outside the Cinema catalog', () => {
  test('a direct file stays gated until Load, then both members hold one position', async ({
    scenario,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Autoplay and media timing are asserted on Chromium.');
    const { a, b, channelId } = await scenario.ownerAndMember();
    const external: string[] = [];
    for (const m of [a, b]) {
      await serveTone(m.page);
      m.page.on('request', (r) => {
        if (r.url().includes('media.sideroom-e2e.test')) external.push(r.url());
      });
    }

    // The host starts a party bound to the channel; the server validates and stores the canonical link.
    const party = await apiPost<{
      id: string;
      media: { kind: string; url: string; provider: string; title_id: string };
    }>(a.context.request, '/watch-parties', {
      channel_id: channelId,
      external: { url: TONE_URL, title: 'Night tone' },
    });
    expect(party.media).toMatchObject({ kind: 'direct', url: TONE_URL, provider: 'direct' });

    await a.page.goto(`/cinema/party/${party.id}`);
    await b.page.goto(`/cinema/party/${party.id}`);

    // Gate: the page names the host and has fetched nothing from it.
    for (const m of [a, b]) {
      await expect(m.page.getByText(/media\.sideroom-e2e\.test/)).toBeVisible();
      await expect(m.page.locator('.wp-ext video')).toHaveCount(0);
    }
    expect(external).toEqual([]);

    // One click each (a user gesture, which also lets sound start).
    for (const m of [a, b]) await m.page.getByRole('button', { name: 'Load' }).click();
    for (const m of [a, b]) await expect(m.page.locator('.wp-ext video')).toHaveCount(1);
    expect(external.length).toBeGreaterThan(0);

    // The host presses play for everyone; both players advance.
    await a.page.getByRole('button', { name: 'Play for everyone' }).click();
    await expect.poll(() => mediaTime(a.page), { timeout: 20_000 }).toBeGreaterThan(1);
    await expect.poll(() => mediaTime(b.page), { timeout: 20_000 }).toBeGreaterThan(1);
    await a.page.waitForTimeout(3_000);
    const [ta, tb] = await Promise.all([mediaTime(a.page), mediaTime(b.page)]);
    expect(Math.abs(ta - tb), `positions ${ta} / ${tb}`).toBeLessThan(0.6);

    // Pause for everyone: both stop.
    await a.page.getByRole('button', { name: 'Pause for everyone' }).click();
    await expect
      .poll(
        async () =>
          (await b.page.evaluate(
            () => document.querySelector<HTMLMediaElement>('.wp-ext video')?.paused,
          )) ?? false,
        {
          timeout: 10_000,
        },
      )
      .toBe(true);
    const frozen = await mediaTime(b.page);
    await b.page.waitForTimeout(1_000);
    expect(Math.abs((await mediaTime(b.page)) - frozen)).toBeLessThan(0.15);
  });

  test('a provider embed loads only after Load, from the privacy-enhanced host, and is driven by postMessage', async ({
    scenario,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Frame timing is asserted on Chromium.');
    const { a, b, channelId } = await scenario.ownerAndMember();
    const hits: string[] = [];
    for (const m of [a, b]) await stubProvider(m.page, hits);

    const party = await apiPost<{ id: string; media: { kind: string; url: string } }>(
      a.context.request,
      '/watch-parties',
      { channel_id: channelId, external: { url: YT_URL } },
    );
    // The link was canonicalised to the no-cookie embed; the page's own host never appears.
    expect(party.media).toMatchObject({
      kind: 'embed',
      url: `${YT_ORIGIN}/embed/dQw4w9WgXcQ`,
    });

    await a.page.goto(`/cinema/party/${party.id}`);
    await b.page.goto(`/cinema/party/${party.id}`);
    for (const m of [a, b]) {
      await expect(m.page.getByText(/www\.youtube-nocookie\.com/)).toBeVisible();
      await expect(m.page.locator('iframe')).toHaveCount(0);
    }
    expect(hits, 'nothing was requested from the provider before the click').toEqual([]);

    for (const m of [a, b]) await m.page.getByRole('button', { name: 'Load' }).click();
    for (const m of [a, b]) await expect(m.page.locator('.wp-ext iframe')).toHaveCount(1);
    const src = new URL((await a.page.locator('.wp-ext iframe').getAttribute('src')) ?? '');
    expect(src.origin).toBe(YT_ORIGIN);
    expect(src.searchParams.get('enablejsapi')).toBe('1');
    expect(src.searchParams.get('controls')).toBe('0');

    // Both frames answered the handshake; the host plays for everyone.
    await expect
      .poll(() => frameOf(a.page)?.evaluate(() => !!window['__fake' as never]))
      .toBe(true);
    await a.page.getByRole('button', { name: 'Play for everyone' }).click();
    for (const m of [a, b]) {
      await expect
        .poll(
          () =>
            frameOf(m.page)?.evaluate(() =>
              (window as never as { __fake: { state(): number } }).__fake.state(),
            ),
          {
            timeout: 20_000,
          },
        )
        .toBe(1);
    }
    await a.page.waitForTimeout(3_000);
    const at = (p: Page) =>
      frameOf(p)!.evaluate(() => (window as never as { __fake: { cur(): number } }).__fake.cur());
    const [ta, tb] = await Promise.all([at(a.page), at(b.page)]);
    expect(Math.abs(ta - tb), `positions ${ta} / ${tb}`).toBeLessThan(1.0);

    // A seek by the host reaches the guest's frame as seekTo.
    await a.page.getByRole('button', { name: 'Forward 10 seconds' }).click();
    await expect
      .poll(
        () =>
          frameOf(b.page)?.evaluate(() =>
            (window as never as { __fake: { commands: string[] } }).__fake.commands.includes(
              'seekTo',
            ),
          ),
        {
          timeout: 10_000,
        },
      )
      .toBe(true);
  });
});
