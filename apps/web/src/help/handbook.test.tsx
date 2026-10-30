// The member handbook at /help (RDY-ONB-002) and "What's new" at /help/whats-new (RDY-ONB-003).
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createAppRouter } from '../router';
import { t } from '../i18n';
import { resetCompliancePublic } from '../lib/compliancePublic';
import { APP_VERSION, hasUnseenNews, markNewsSeen } from '../lib/whatsNew';
import { http, HttpResponse, server } from '../test/server';
import { HANDBOOK_SECTIONS } from './Handbook';
import { changelogBody } from './WhatsNew';

const root = resolve(__dirname, '../../../..');
const changelog = readFileSync(resolve(root, 'CHANGELOG.md'), 'utf8');

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  resetCompliancePublic();
  localStorage.clear();
});

describe('the handbook', () => {
  it('has a section for every topic the roadmap names, each with a heading', async () => {
    at('/help');
    expect(await screen.findByRole('heading', { level: 1, name: 'Help' })).toBeVisible();
    expect(HANDBOOK_SECTIONS.map((s) => s.id)).toEqual([
      'joining',
      'signin',
      'install',
      'lounge',
      'submit',
      'parties',
      'tv',
      'privacy',
      'house',
    ]);
    for (const s of HANDBOOK_SECTIONS) {
      expect(screen.getByRole('heading', { level: 2, name: t(s.title) })).toBeVisible();
    }
  });

  it('every section links the screen it describes, and each link is a real route', async () => {
    const router = at('/help');
    await screen.findByRole('heading', { level: 1, name: 'Help' });
    for (const s of HANDBOOK_SECTIONS) {
      expect(s.links.length, s.id).toBeGreaterThan(0);
      const section = document.getElementById(s.id)!;
      for (const l of s.links) {
        const link = within(section).getByRole('link', { name: t(l.label) });
        expect(link).toHaveAttribute('href', l.href);
        const path = l.href.split('#')[0]!;
        // The house notes are a plain page of the public build, not a route of the app.
        if (path === '/house-notes') continue;
        const matches = router.matchRoutes(path, {});
        const leaf = matches[matches.length - 1];
        expect(leaf?.routeId, `${l.href} is not a route`).not.toBe('__root__');
        expect(leaf?.status, l.href).not.toBe('notFound');
      }
    }
  });

  it('says how many credits a new account gets, live, with a fallback while it is not known', async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({ minimum_age: 13, invite_initial_credits: 8 }),
      ),
    );
    at('/help');
    expect(await screen.findByText(/every account starts with 8 invite credits/)).toBeVisible();
  });

  it('uses the plain wording while the number is unknown or the server is unreachable', async () => {
    server.use(http.get('*/api/v1/sr/compliance/public', () => HttpResponse.error()));
    at('/help');
    expect(
      await screen.findByText(/a handful of invite credits \(five by default\)/),
    ).toBeVisible();
    expect(screen.queryByText(/invite credits, and one credit lets/)).toBeNull();
    // The sentence's screen: the Invites settings, which is a registered route.
    expect(screen.getByRole('link', { name: 'Open Invites' })).toHaveAttribute(
      'href',
      '/settings/user/invites',
    );
  });

  it('agrees with doc 09 on what is stored and for how long', () => {
    const doc = readFileSync(resolve(root, 'docs/09-retention-and-privacy.md'), 'utf8');
    const privacy = [
      t('help.handbook.privacy.stored'),
      t('help.handbook.privacy.export'),
      t('help.handbook.lounge.diff2'),
    ].join(' ');
    // Each figure the handbook states, and the sentence of doc 09 it comes from.
    const facts: [RegExp, RegExp][] = [
      [/purged after 30 days/, /Purged 30 days later/],
      [/kept for 180 days|Sign-in addresses are kept for 180 days/, /Login IP history 180 days/],
      [/download link for seven days/, /download link for 7 days/],
      [/Files over 30 MB expire seven days/, /Attachments > 30 MB[^\n]*\*\*Expire 7 days/],
      [/Voice and video are never recorded/, /Never recorded or stored/],
    ];
    for (const [handbook, source] of facts) {
      expect(privacy + t('help.handbook.lounge.diff1'), String(handbook)).toMatch(handbook);
      expect(doc, String(source)).toMatch(source);
    }
    // What stays after deletion is the list in doc 09 section 1, in the handbook's words.
    for (const kept of ['invite graph', 'invoices', 'moderation history', 'Deleted User']) {
      expect(t('help.handbook.privacy.delete')).toContain(kept);
    }
    expect(doc).toMatch(/invite graph, invoices, staff-side moderation history/);
    expect(doc).toMatch(/Deleted User/);
  });

  it("names the operator's country and contact when they are set", async () => {
    server.use(
      http.get('*/api/v1/sr/compliance/public', () =>
        HttpResponse.json({
          minimum_age: 13,
          operator_country: 'DE',
          operator_contact: 'legal@example.org',
        }),
      ),
    );
    at('/help');
    const privacy = await screen.findByRole('heading', { level: 2, name: /^Privacy:/ });
    const section = privacy.closest('section')!;
    expect(await within(section).findByText('This server is run from DE.')).toBeVisible();
    expect(within(section).getByRole('link', { name: 'legal@example.org' })).toBeVisible();
  });

  it('offers the welcome again only to a signed-in member, and stores that it is open', async () => {
    const puts: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '5', first_run_done: true })),
      http.put('*/api/v1/sr/auth/first-run', async ({ request }) => {
        puts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const router = at('/help');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Show the welcome again' }));
    await waitFor(() => expect(puts).toEqual([{ done: false }]));
    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/channels/));
  });

  it('shows no welcome section to a visitor who is not signed in', async () => {
    at('/help');
    await screen.findByRole('heading', { level: 1, name: 'Help' });
    await new Promise((r) => setTimeout(r, 100));
    expect(screen.queryByRole('button', { name: 'Show the welcome again' })).toBeNull();
  });
});

describe("What's new", () => {
  it('renders the changelog newest first and leaves out the note to the integrator', async () => {
    at('/help/whats-new');
    expect(await screen.findByRole('heading', { level: 1, name: "What's new" })).toBeVisible();
    const phases = screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent);
    expect(phases[0]).toBe(changelog.match(/^## (.+)$/m)![1]);
    const numbers = phases.map((p) => Number(/Phase (\d+)/.exec(p ?? '')?.[1]));
    expect(numbers).toEqual([...numbers].sort((a, b) => b - a));
    expect(screen.queryByText(/The integrator adds the new wave/)).toBeNull();
    // A wrapped bullet is one list item, not a stray paragraph.
    expect(
      screen.getByText(/Submit a show or film from the browser\./).closest('li'),
    ).toHaveTextContent(/Large uploads continue where they stopped/);
  });

  it('is its own page reachable from the handbook', async () => {
    const router = at('/help');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('link', { name: "What's new in SIDE ROOM" }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/help/whats-new'));
  });

  it('opening it clears the badge for this version', async () => {
    localStorage.setItem('sr.whatsNew.seen', 'Phase 1');
    expect(hasUnseenNews()).toBe(true);
    at('/help/whats-new');
    await screen.findByRole('heading', { level: 1, name: "What's new" });
    expect(localStorage.getItem('sr.whatsNew.seen')).toBe(APP_VERSION);
    expect(hasUnseenNews()).toBe(false);
  });
});

describe('the version the build embeds', () => {
  it('is the newest heading of CHANGELOG.md without its date', () => {
    const heading = /^## (.+?)\s*$/m.exec(changelog)![1]!;
    expect(APP_VERSION).toBe(heading.replace(/\s+-\s+\d{4}-\d{2}-\d{2}$/, ''));
    expect(APP_VERSION).toMatch(/^Phase \d+$/);
  });

  it('the badge shows once per version: never to a new install, once after a change', () => {
    // A browser that has seen nothing is a new install: no badge, and the version is recorded.
    expect(hasUnseenNews('Phase 22')).toBe(false);
    expect(localStorage.getItem('sr.whatsNew.seen')).toBe('Phase 22');
    expect(hasUnseenNews('Phase 22')).toBe(false);
    // The version changes: the badge shows until the news is opened, then not again.
    expect(hasUnseenNews('Phase 23')).toBe(true);
    expect(hasUnseenNews('Phase 23')).toBe(true);
    markNewsSeen('Phase 23');
    expect(hasUnseenNews('Phase 23')).toBe(false);
  });

  it('changelogBody starts at the first wave and joins wrapped bullets', () => {
    const body = changelogBody('# Changelog\n\nnote\n\n## Phase 2\n\n- one\n  wrapped\n- two\n');
    expect(body.startsWith('## Phase 2')).toBe(true);
    expect(body).toContain('- one wrapped\n- two');
    expect(changelogBody('nothing here')).toBe('');
  });
});
