import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { repoRoot } from '../support/cli';
import { bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-065 (D-595): the takedown detail names the fingerprint state of each delisted file. Opt-in visual check
 * (SR_E2E_SWEEP=1): the Owner opens Admin > Moderation > Takedowns > a notice whose items cover every state (the
 * notice endpoints are answered in the page: a real storage takedown needs a connected bucket), at desktop and
 * phone width in Lounge Light and Editorial, and the page is screenshotted to SR_SWEEP_OUT.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const OUT = process.env.SR_SWEEP_OUT ?? resolve(repoRoot, '..', 'visual-sweep-shots', 'p58-store');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

test.skip(!ON, 'Opt-in visual check: set SR_E2E_SWEEP=1.');
test.use({ bypassCSP: true });

const SUMMARY = {
  id: '5001',
  kind: 'copyright',
  state: 'removed',
  notifier_name: 'Rights Holder Ltd',
  received_at_ms: 1_700_000_000_000,
  triage_due_at_ms: 1_700_086_400_000,
  triage_overdue: false,
  case_id: null,
  items: 5,
  removed_at_ms: 1_700_000_100_000,
  restore_after_ms: null,
  court_filing_noted: false,
};
const file = (id: string, digest: string) => ({
  id,
  kind: 'storage_object',
  target_id: `90${id}`,
  label: `Object films/leak-${id}.mkv in bucket Shelf`,
  object_key: `films/leak-${id}.mkv`,
  subject_account_id: '71',
  state: 'disabled',
  hold_id: null,
  disabled_at_ms: 1_700_000_100_000,
  restored_at_ms: null,
  digest_state: digest,
});
const DETAIL = {
  ...SUMMARY,
  notifier_contact: 'legal@rights.example',
  legal_basis: 'Copyright in the film',
  locations: [],
  original_text: 'Please remove it.',
  entered_by: '1',
  rejection_reason: '',
  counter_notice_text: '',
  counter_noticed_at_ms: null,
  restored_at_ms: null,
  restore_reason: '',
  items: [
    file('1', 'recorded'),
    file('2', 'pending'),
    file('3', 'waiting_mount'),
    file('4', 'tag_only'),
    {
      ...file('5', 'tag_only'),
      kind: 'library_series',
      object_key: undefined,
      digest_state: undefined,
    },
  ],
  events: [
    { id: '1', actor_id: '1', kind: 'received', detail: {}, created_at_ms: 1_700_000_000_000 },
  ],
  strikes: [],
};

const VIEWPORTS = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;

for (const [vp, size] of Object.entries(VIEWPORTS)) {
  for (const theme of ['lounge-light', 'lounge-editorial']) {
    test(`takedown detail ${vp} ${theme}`, async ({ browser }) => {
      mkdirSync(OUT, { recursive: true });
      const context = await browser.newContext({
        baseURL: ORIGIN,
        viewport: size,
        colorScheme: theme === 'lounge-light' ? 'light' : 'dark',
        bypassCSP: true,
      });
      await context.addInitScript((t) => {
        try {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }),
          );
        } catch {
          /* no storage */
        }
      }, theme);
      await loginApi(context.request, bootstrapState().owner);
      const page = await context.newPage();
      await page.route(/\/api\/v1\/moderation\/takedowns(\?.*)?$/, (r) =>
        r.fulfill({ json: [SUMMARY] }),
      );
      await page.route(/\/api\/v1\/moderation\/takedowns\/5001(\?.*)?$/, (r) =>
        r.fulfill({ json: DETAIL }),
      );
      await page.goto('/admin/moderation');
      await page.getByRole('tab', { name: 'Takedowns' }).click();
      await page.getByRole('button', { name: 'Open takedown notice 5001' }).click();
      await expect(page.getByRole('heading', { name: 'Takedown notice TK-5001' })).toBeVisible();
      await expect(page.getByText('Fingerprint: recorded')).toBeVisible();
      await expect(
        page.getByText('Fingerprint: waiting, the owner has switched this bucket off'),
      ).toBeVisible();
      const list = page.getByRole('list', { name: /content/i }).first();
      await list.scrollIntoViewIfNeeded();
      await page.screenshot({
        path: resolve(OUT, `takedown-detail-${vp}-${theme}.png`),
        fullPage: true,
      });
      await context.close();
    });
  }
}
