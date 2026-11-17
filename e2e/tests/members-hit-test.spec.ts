import type { Page } from '@playwright/test';
import { apiPost, expect, test } from '../support/fixtures';

/**
 * SEC-AUD-400 (D-912): Server Settings, Members keeps a touch target on its row checkbox, and the checkbox works.
 * D-898 accepted Chromium-only proof for the kit `Checkbox` here on the promise that CI would cover the other
 * engines; this spec is that cover. It always runs (no switch), makes its own server and three members through the
 * API (nothing from the sweep's world), and is meant to run in chromium, firefox and webkit.
 *
 * Phone width is a viewport only (no isMobile, no hasTouch: Firefox refuses them, and the kit's phone rules key on
 * width). It asserts behaviour only: where `document.elementFromPoint` lands 21 px around the box's centre (D-856),
 * never a class name or a pixel size.
 *
 * Command (from `e2e/`, on a stack with the Owner bootstrapped by global setup):
 *   SR_E2E_BASE_URL=http://localhost:5695 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   npx playwright test tests/members-hit-test.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */

interface Probe {
  label: string;
  hits: {
    at: string;
    tag: string;
    own: boolean;
    label: boolean;
    interactive: boolean;
    path: string;
  }[];
  controls: { name: string; centreOwn: boolean; path: string }[];
  scrollOverflow: number;
}

/** In the page: what each of five points around the row's checkbox hits, and what its row's other controls do. */
function probeCheckbox(name: string): Probe {
  const all = Array.from(document.querySelectorAll<HTMLInputElement>('input[type=checkbox]'));
  const input = all.find((e) => e.getAttribute('aria-label') === name);
  if (!input) throw new Error(`no checkbox named ${name}`);
  input.scrollIntoView({ block: 'center' });
  const path = (e: Element | null) => {
    const parts: string[] = [];
    for (let n: Element | null = e; n && parts.length < 4; n = n.parentElement) {
      const cls = typeof n.className === 'string' ? n.className.trim().split(/\s+/)[0] : '';
      parts.push(n.tagName.toLowerCase() + (cls ? `.${cls.slice(0, 24)}` : ''));
    }
    return parts.join(' < ');
  };
  const wrapper = input.parentElement;
  const lab =
    input.closest('label') ??
    (input.id ? document.querySelector(`label[for="${input.id}"]`) : null);
  const INTERACTIVE = 'a, button, select, input, textarea, [role=button], [role=menuitem]';
  const r = input.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;
  const points: [string, number, number][] = [
    ['centre', cx, cy],
    ['above', cx, cy - 21],
    ['below', cx, cy + 21],
    ['left', cx - 21, cy],
    ['right', cx + 21, cy],
  ];
  const hits = points.map(([at, x, y]) => {
    const h = document.elementFromPoint(x, y);
    const own = !!h && (h === input || input.contains(h) || (!!wrapper && wrapper.contains(h)));
    const label = !!h && !!lab && (h === lab || lab.contains(h));
    const hi = h?.closest(INTERACTIVE) ?? null;
    // "Interactive" here means a control other than this checkbox (or its label) took the point.
    const interactive = !!hi && hi !== input && !(lab && (hi === lab || lab.contains(hi)));
    return {
      at,
      tag: h ? h.tagName.toLowerCase() : 'none',
      own,
      label,
      interactive,
      path: path(h),
    };
  });
  const row = input.closest('tr');
  const controls = Array.from(row?.querySelectorAll<HTMLElement>(INTERACTIVE) ?? [])
    .filter((e) => e !== input && e.getBoundingClientRect().width > 0)
    .map((e) => {
      const b = e.getBoundingClientRect();
      const h = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
      return {
        name: (e.getAttribute('aria-label') || e.textContent || e.tagName).trim().slice(0, 30),
        centreOwn: !!h && (h === e || e.contains(h)),
        path: path(h),
      };
    });
  const de = document.documentElement;
  return { label: name, hits, controls, scrollOverflow: de.scrollWidth - de.clientWidth };
}

async function openMembers(page: Page, guildId: string) {
  // Deep-link to the section. The bare /settings/server/:id route redirects to the first section once the
  // viewer has loaded; that remounts SettingsLayout and closes a phone nav opened meanwhile (Firefox and
  // WebKit lose that race, Chromium happens to win it).
  await page.goto(`/settings/server/${guildId}/members`);
  const dialog = page.getByRole('dialog');
  // On a phone the section list sits behind the "Sections" button; on a desktop it is already open.
  const sections = dialog.getByRole('button', { name: /^Sections/ });
  await expect(dialog).toBeVisible();
  // The button only exists below the phone breakpoint: wait for it there, never on a desktop.
  const members = dialog.getByRole('button', { name: /^Members$/ }).first();
  if ((page.viewportSize()?.width ?? 1280) <= 720) {
    // Retry open-then-look until the nav is open and its list is populated (it fills in after the viewer loads).
    await expect(async () => {
      if ((await sections.getAttribute('aria-expanded')) !== 'true') await sections.click();
      await expect(members).toBeVisible({ timeout: 2_000 });
    }).toPass({ timeout: 30_000 });
  }
  await members.click();
  await expect(page.locator('table.ss-table tbody tr').first()).toBeVisible();
}

test('Members: the row checkbox keeps its touch target and works', async ({
  scenario,
  seedUser,
  signedIn,
}) => {
  test.setTimeout(60_000);
  const { a, b, guildId, channelId } = await scenario.ownerAndMember();
  // A third person, so there are three rows.
  const invite = await apiPost<{ code: string }>(
    a.context.request,
    `/channels/${channelId}/invites`,
    {},
  );
  const c = await signedIn(await seedUser('third'));
  await apiPost(c.context.request, `/invites/${invite.code}`);
  await b.context.close();
  await c.context.close();

  const page = a.page;

  // Phone width: viewport only.
  await page.setViewportSize({ width: 375, height: 812 });
  await openMembers(page, guildId);
  const rows = page.locator('table.ss-table tbody tr');
  await expect(rows).toHaveCount(3);
  const first = page.getByRole('checkbox', { name: /^Select (?!all)/ }).first();
  await expect(first).toBeVisible();
  const name = (await first.getAttribute('aria-label')) ?? '';
  expect(name).toMatch(/^Select \S/);
  await first.scrollIntoViewIfNeeded();

  const probe = await page.evaluate(probeCheckbox, name);
  const where = (p: Probe) =>
    p.hits.map((h) => `${h.at}: ${h.tag} (${h.path})${h.own || h.label ? '' : ' MISS'}`).join('\n');
  // Each point lands on the input, a node inside its wrapper, or its own label ...
  for (const h of probe.hits) {
    expect(h.own || h.label, `point ${h.at} of "${name}" was taken by:\n${where(probe)}`).toBe(
      true,
    );
    // ... and no other interactive control of the row takes any of the five points.
    expect(h.interactive, `point ${h.at} of "${name}" hit another control:\n${where(probe)}`).toBe(
      false,
    );
  }
  // Every other control of the row (name, role and action buttons) returns itself at its own centre.
  expect(probe.controls.length, 'the row has other controls to compare').toBeGreaterThan(0);
  for (const ctl of probe.controls) {
    expect(ctl.centreOwn, `control "${ctl.name}" lost its own centre to ${ctl.path}`).toBe(true);
  }
  expect(probe.scrollOverflow, 'no sideways scroll at 375').toBeLessThanOrEqual(0);

  // It works: a click checks it and shows the bulk bar; focus and Space clear both again.
  const bar = page.getByRole('toolbar');
  await expect(bar).toHaveCount(0);
  await first.click();
  await expect(first).toBeChecked();
  await expect(bar).toBeVisible();
  await first.focus();
  await page.keyboard.press('Space');
  await expect(first).not.toBeChecked();
  await expect(bar).toHaveCount(0);

  // Desktop: "select all" exists (it is hidden on a phone, D-890), checks every row, and a second click clears.
  await page.setViewportSize({ width: 1280, height: 800 });
  const all = page.getByRole('checkbox', { name: /^Select all/ });
  await expect(all).toBeVisible();
  const rowBoxes = page.getByRole('checkbox', { name: /^Select (?!all)/ });
  await expect(rowBoxes).toHaveCount(3);
  await all.click();
  await expect(all).toBeChecked();
  for (let i = 0; i < 3; i++) await expect(rowBoxes.nth(i)).toBeChecked();
  await expect(bar).toBeVisible();
  await all.click();
  for (let i = 0; i < 3; i++) await expect(rowBoxes.nth(i)).not.toBeChecked();
  await expect(all).not.toBeChecked();
});
