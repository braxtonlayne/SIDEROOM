// SEC-AUD-195 (D-647, D-665): the 100-accounts notice in Admin > Bug reports.
import { screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { BETA_REPORTS_FLAG } from '../../bugreport/flag';
import { setClientFlags } from '../../moderation/reports';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

beforeEach(() => setClientFlags({ [BETA_REPORTS_FLAG]: true }));
afterEach(() => setClientFlags({}));

function listAnswers(beta_over_100: boolean) {
  server.use(
    http.get('*/api/v1/admin/bug-reports', () =>
      HttpResponse.json({ reports: [], open_count: 0, beta_over_100 }),
    ),
  );
}

describe('Admin > Bug reports: the beta notice', () => {
  it('warns above the list, with a link to Features for staff who manage features', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    listAnswers(true);
    openAdmin('bug-reports');
    expect(await screen.findByText(/this platform has 100 or more accounts/)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Open Features' })).toHaveAttribute(
      'href',
      '/admin/features',
    );
  });

  it('has no link for staff who cannot see Features', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    listAnswers(true);
    openAdmin('bug-reports');
    expect(await screen.findByText(/this platform has 100 or more accounts/)).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Open Features' })).toBeNull();
  });

  it('is not shown while the answer says false', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    listAnswers(false);
    openAdmin('bug-reports');
    await screen.findByText('No reports');
    expect(screen.queryByText(/100 or more accounts/)).toBeNull();
  });
});
