import { expect, test, apiPost, apiPostBytes, apiPut, loginApi } from '../support/fixtures';
import { builtInActivityBundle, testActivityBundle } from '../support/activityBundle';
import { startActivityForTwo } from '../support/activities';
import { execFileSync } from 'node:child_process';
import { DATABASE_URL, runCli } from '../support/cli';
import { VOICE_STACK } from '../support/voice';

/**
 * ARC-SBX-001 (wave 42 gate proof), written to be run by the wave-42 integrator: NOT run by its author (ports
 * 5173/8080 belong to the wave-41 integrator).
 *
 * Land.io runs in a voice call from the sandbox origin (`<app-id>.activities.localhost`):
 *  1. the iframe is `allow-scripts allow-pointer-lock` only and its origin is opaque;
 *  2. from inside it nothing reaches a non-allow-listed origin (CSP `connect-src 'none'`), not even its own host;
 *     cookies and the parent page are out of reach;
 *  3. its settings still persist, through the bridge's `storage` capability;
 *  4. the operator signs the code (`sr-cli arcade sign-code`): the badge says SWARM-SAFETY-SIGNED;
 *  5. a member vouches; when the developer ships different code the badge flips to UNVERIFIED UPDATE with the
 *     previous build's vouch count.
 *
 * Needs the voice stack, `SR_ACTIVITIES__ENABLED=true` and an `sr-cli` that shares the server's database and storage
 * (the signature is made with the real operator tool). The Owner account (old enough to be in good standing)
 * is the voucher. Set SR_E2E_ACTIVITIES=1.
 */
const ON = process.env.SR_E2E_ACTIVITIES === '1';

test.use({
  launchOptions: {
    args: [
      '--use-fake-device-for-media-stream',
      '--use-fake-ui-for-media-stream',
      '--disable-features=WebRtcHideLocalIpsWithMdns',
      '--allow-loopback-in-peer-connection',
    ],
  },
});

interface Trust {
  code_sha256: string | null;
  headline: 'operator_featured' | 'swarm_safety_signed' | 'unverified_update' | 'unsigned';
  vouches: number;
  can_vouch: boolean;
  previous: { vouches: number; short: string } | null;
}

test.describe('Arcade sandbox (real browser)', () => {
  test.skip(!ON || !VOICE_STACK, 'Set SR_E2E_ACTIVITIES=1 and SR_E2E_VOICE=1.');

  test('Land.io cannot leave its sandbox; signed badge; vouch flips on new code', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium resolves *.localhost by itself.');
    const session = await startActivityForTwo(
      { browser, browserName, owner, playwright },
      builtInActivityBundle('landio'),
      'sbx',
    );
    try {
      const { a, appId, name } = session;
      const host = `${appId}.activities.localhost`;
      const iframe = a.page.locator(`iframe[title="${name} (Activity)"]`);

      // 1. The sandbox attribute is exactly scripts + pointer lock; the document's origin is opaque.
      expect(await iframe.getAttribute('sandbox')).toBe('allow-scripts allow-pointer-lock');
      const fa = a.page.frames().find((f) => f.url().includes(host))!;
      expect(await fa.evaluate(() => self.origin)).toBe('null');
      await expect(session.a.frame.locator('#start')).toBeVisible({ timeout: 60_000 });

      // 2. Nothing leaves. CSP violations are reported by the browser; every fetch rejects.
      const probe = await fa.evaluate(async () => {
        const violations: string[] = [];
        document.addEventListener('securitypolicyviolation', (e) =>
          violations.push(e.effectiveDirective),
        );
        const tries: Record<string, string> = {};
        for (const [label, url] of Object.entries({
          foreign: 'https://example.invalid/steal',
          app: `${location.protocol}//localhost:5173/api/v1/sr/csrf`,
          self: location.href,
        })) {
          try {
            await fetch(url, { mode: 'no-cors' });
            tries[label] = 'reached';
          } catch {
            tries[label] = 'blocked';
          }
        }
        // The violation report is queued as its own task: give it a moment to arrive.
        await new Promise((r) => setTimeout(r, 250));
        let cookie = 'readable';
        try {
          void document.cookie;
        } catch {
          cookie = 'blocked';
        }
        let parentDoc = 'readable';
        try {
          void window.parent.document.title;
        } catch {
          parentDoc = 'blocked';
        }
        return { tries, cookie, parentDoc, violations };
      });
      expect(probe.tries).toEqual({ foreign: 'blocked', app: 'blocked', self: 'blocked' });
      expect(probe.cookie).toBe('blocked');
      expect(probe.parentDoc).toBe('blocked');
      expect(probe.violations).toContain('connect-src');

      // 3. A setting survives through the bridge: the sandbox has no localStorage of its own, the SDK installs one.
      const stored = await fa.evaluate(() => {
        localStorage.setItem('sbx.check', '1');
        return localStorage.getItem('sbx.check');
      });
      expect(stored).toBe('1');

      // 4. The operator signs the code of this exact build.
      const staff = await playwright.request.newContext({
        baseURL: process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173',
      });
      await loginApi(staff, owner);
      const trustOf = async (): Promise<Trust> =>
        (await (await staff.get(`/api/v1/activities/${appId}/trust`)).json()) as Trust;
      const t0 = await trustOf();
      expect(t0.headline).toBe('unsigned');
      const hash = t0.code_sha256!;
      runCli(['arcade', 'sign-code', '--hash', hash, '--application', appId]);
      expect((await trustOf()).headline).toBe('swarm_safety_signed');

      // 5. The Owner vouches, pinned to the hash. Vouching needs an account at least 3 days old (D-405) and CI
      // creates the owner minutes earlier, so age it first.
      execFileSync('psql', [
        DATABASE_URL,
        '-X',
        '-q',
        '-c',
        `UPDATE accounts SET created_at = now() - interval '30 days' WHERE id = ${BigInt(owner.id)}`,
      ]);
      const vouched = await apiPut<Trust>(staff, `/activities/${appId}/vouch`, {
        code_sha256: hash,
      });
      expect(vouched.vouches).toBe(1);

      // The developer (Bob, the application's owner) ships different code and staff approve it.
      const bob = session.b.context.request;
      const up = await apiPostBytes<{ bundle: { id: string } }>(
        bob,
        `/applications/${appId}/activity/bundles`,
        testActivityBundle(),
        'application/zip',
      );
      await apiPost(bob, `/applications/${appId}/activity/submit`, { bundle_id: up.bundle.id });
      await apiPost(staff, `/admin/developers/activities/${appId}/decision`, {
        decision: 'approve',
        bundle_id: up.bundle.id,
      });
      const flipped = await trustOf();
      expect(flipped.code_sha256).not.toBe(hash);
      expect(flipped.headline).toBe('unverified_update');
      expect(flipped.vouches).toBe(0);
      expect(flipped.previous?.vouches).toBe(1);
      expect(flipped.previous?.short).toBe(hash.slice(0, 8));

      // A vouch for the replaced build is refused.
      await expect(
        apiPut(staff, `/activities/${appId}/vouch`, { code_sha256: hash }),
      ).rejects.toThrow(/409/);
      await staff.dispose();
    } finally {
      await session.close();
    }
  });
});
