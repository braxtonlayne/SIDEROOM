// T-03 (D-369): Admin > Runbook renders deploy/RUNBOOK.md with the playbook anchors, and the System tiles open it.
import { readFileSync } from 'node:fs';
import { render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
// @ts-expect-error plain ES module without types
import { headingSlugs, REQUIRED_PLAYBOOKS, slug } from '../../../../../scripts/lib/playbooks.mjs';
import runbook from '../../../../../deploy/RUNBOOK.md?raw';
import { EntranceMarkdown } from '../../routes/entrance/entranceCopy';
import { runbookBody, runbookSlug } from '../sections/Runbook';
import { asStaff, http, HttpResponse, nav, openAdmin, server } from './helpers';

const required = REQUIRED_PLAYBOOKS as string[];

describe('runbook anchors', () => {
  it('runbookSlug is the slug rule of scripts/lib/playbooks.mjs', () => {
    const samples = [
      'Server down',
      'A member reports CSAM',
      'Data-subject request received',
      'Upgrade note: legacy download fallback is off',
      'Staff alerts and the second-factor window',
      'The `ops.backup_dir` setting (été) & more!',
      '  spaced   out  ',
      ...(runbook.match(/^#{1,6} .*$/gm) ?? []).map((h) => h.replace(/^#+\s+/, '')),
    ];
    for (const s of samples) expect(runbookSlug(s)).toBe((slug as (h: string) => string)(s));
  });

  it('every heading of the file gets the id the ops check expects', () => {
    const { container } = render(
      <EntranceMarkdown source={runbookBody(runbook)} headingId={runbookSlug} />,
    );
    const ids = new Set(
      [...container.querySelectorAll('h2, h3, h4')].map((h) => h.getAttribute('id')),
    );
    const expected = [...(headingSlugs(runbook) as Set<string>)].filter((a) => a !== 'runbook');
    for (const a of expected) expect(ids, a).toContain(a);
    for (const a of required) expect(ids, a).toContain(a);
    expect(required).toHaveLength(22);
  });

  it('renders code blocks, quotes and the settings table instead of raw markup', () => {
    const { container } = render(
      <EntranceMarkdown source={runbookBody(runbook)} headingId={runbookSlug} />,
    );
    expect(container.querySelector('pre code')?.textContent).toContain('sr-cli rotate-secret-key');
    expect(container.querySelector('blockquote')).not.toBeNull();
    const table = container.querySelector('table');
    expect(table).not.toBeNull();
    expect(within(table as HTMLElement).getByRole('columnheader', { name: 'Setting' })).toBeTruthy();
    expect(container.textContent).not.toContain('```');
    expect(container.textContent).not.toContain('|---');
    // Same-page links work; links to other files of the checkout stay text.
    const anchor = within(container).getAllByRole('link', { name: 'Lost owner second factor' })[0];
    expect(anchor).toHaveAttribute('href', '#lost-owner-second-factor');
  });
});

const COUNTS = {
  accounts_active: 7,
  accounts_banned: 0,
  accounts_deactivated: 1,
  accounts_deleted: 0,
  jobs_dead: 3,
  jobs_queued: 0,
  jobs_running: 0,
  sessions_active: 4,
  staff: 1,
};
const RED = {
  counts: COUNTS,
  gateway_sessions: 2,
  probes: [{ name: 'storage', ok: false, error: 'connection refused' }],
  version: '1',
  integrations: [],
  backups: { last_run_ms: null, last_verify_ms: null, snapshot_ms: null },
  disk: [{ path: '/var/lib', total_bytes: 1000, free_bytes: 100 }],
};

describe('Admin > Runbook', () => {
  it('opens for OPERATOR, focuses the playbook the hash names, and the sidebar lists it', async () => {
    asStaff({ perms: ['OPERATOR'] });
    const { router } = openAdmin('runbook#wal-archive-lag');
    const heading = await screen.findByRole(
      'heading',
      { name: 'WAL archive lag' },
      { timeout: 10_000 },
    );
    expect(heading).toHaveAttribute('id', 'wal-archive-lag');
    await waitFor(() => expect(document.activeElement).toBe(heading));
    for (const a of required) expect(document.getElementById(a), a).not.toBeNull();
    expect(router.state.location.pathname).toBe('/admin/runbook');
  });

  it('is hidden without OPERATOR', async () => {
    asStaff({ perms: ['MANAGE_ACCOUNTS'] });
    const { router } = openAdmin('runbook');
    await waitFor(() => expect(router.state.location.pathname).not.toBe('/admin/runbook'), {
      timeout: 10_000,
    });
    expect(within(nav()).queryByText('Runbook')).toBeNull();
    expect(screen.queryByRole('heading', { name: 'WAL archive lag' })).toBeNull();
  });

  it('the five System tile links open /admin/runbook#<anchor> in the same tab', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/system', () => HttpResponse.json(RED)),
      http.get('*/api/v1/admin/system/csp-reports', () =>
        HttpResponse.json({ directives: [], origins: [], paths: [] }),
      ),
    );
    openAdmin('system');
    await screen.findByText('Readiness probes');
    const link = (name: string) => screen.getByRole('link', { name });
    const cases: [string, string][] = [
      ['Playbook for storage', 'server-down'],
      ['Playbook for Dead jobs', 'jobs-failing'],
      ['Playbook for Last backup', 'backup-missing-or-stale'],
      ['Playbook for Last restore check', 'restore-drill-failed'],
      ['Playbook for /var/lib', 'disk-pressure'],
    ];
    for (const [name, anchor] of cases) {
      const a = link(name);
      expect(a.getAttribute('href')).toBe(`/admin/runbook#${anchor}`);
      expect(a).not.toHaveAttribute('target');
      expect(required).toContain(anchor);
    }
    // No link leaves the app for GitHub any more.
    const system = readFileSync('src/admin/sections/System.tsx', 'utf8');
    expect(system).not.toMatch(/github\.com/);
  });
});
