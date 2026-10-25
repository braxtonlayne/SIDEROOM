// RDY-DOC-003: red tiles on Admin > System link to their section of deploy/RUNBOOK.md, opened in Admin (T-03);
// healthy tiles show no link.
import { screen } from '@testing-library/react';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const COUNTS = {
  accounts_active: 7,
  accounts_banned: 0,
  accounts_deactivated: 1,
  accounts_deleted: 0,
  jobs_dead: 0,
  jobs_queued: 0,
  jobs_running: 0,
  sessions_active: 4,
  staff: 1,
};
const HEALTHY = {
  counts: COUNTS,
  gateway_sessions: 2,
  probes: [{ name: 'database', ok: true }],
  version: '1',
  integrations: [],
  backups: { last_run_ms: 1_800_000_000_000, last_verify_ms: 1_800_000_000_000, snapshot_ms: null },
  disk: [{ path: '/var/lib', total_bytes: 1000, free_bytes: 900 }],
};
const CSP = { directives: [], origins: [], paths: [] };

function serve(system: Record<string, unknown>) {
  asStaff({ perms: [], owner: true });
  server.use(
    http.get('*/api/v1/admin/system', () => HttpResponse.json(system)),
    http.get('*/api/v1/admin/system/csp-reports', () => HttpResponse.json(CSP)),
  );
  openAdmin('system');
}

describe('System tile playbooks', () => {
  it('shows no playbook link while everything is healthy', async () => {
    serve(HEALTHY);
    await screen.findByText('Readiness probes');
    expect(screen.queryAllByRole('link', { name: /^Playbook/ })).toHaveLength(0);
  });

  it('links each red tile to the RUNBOOK section named in scripts/check-ops-files.mjs', async () => {
    serve({
      ...HEALTHY,
      counts: { ...COUNTS, jobs_dead: 3 },
      probes: [{ name: 'storage', ok: false, error: 'connection refused' }],
      backups: { last_run_ms: null, last_verify_ms: null, snapshot_ms: null },
      disk: [
        { path: '/var/lib', total_bytes: 1000, free_bytes: 100 },
        { path: '/srv/ingest', total_bytes: null, free_bytes: null },
      ],
    });
    await screen.findByText('Readiness probes');
    const href = (name: string) => screen.getByRole('link', { name }).getAttribute('href');
    expect(href('Playbook for storage')).toBe('/admin/runbook#server-down');
    expect(href('Playbook for Dead jobs')).toBe('/admin/runbook#jobs-failing');
    expect(href('Playbook for Last backup')).toBe('/admin/runbook#backup-missing-or-stale');
    expect(href('Playbook for Last restore check')).toBe('/admin/runbook#restore-drill-failed');
    expect(href('Playbook for /var/lib')).toBe('/admin/runbook#disk-pressure');
    expect(href('Playbook for /srv/ingest')).toBe('/admin/runbook#disk-pressure');
    // The object snapshot is optional: "none" there is not red.
    expect(screen.queryByRole('link', { name: 'Playbook for Last object snapshot' })).toBeNull();
  });
});
