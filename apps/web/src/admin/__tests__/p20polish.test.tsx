// P19 follow-up (MOD-DASH-002, D-339): the Review hub's Reports tile counts unattached reports for MANAGE_CASES.
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const SLOW = { timeout: 10_000 };

describe('Review hub Reports tile', () => {
  it('shows the unattached reports to MANAGE_CASES holders and links to the dashboard', async () => {
    let asked = 0;
    server.use(
      http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/dashboard/inbox/count', () => {
        asked += 1;
        return HttpResponse.json({ groups: 2, reports: 5, oldest_seconds: 120 });
      }),
    );
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES', 'MANAGE_CASES'] });
    openAdmin('review');
    const link = await screen.findByRole('link', { name: /5 reports are waiting/ }, SLOW);
    expect(link).toHaveAttribute('href', '/admin/moderation?tab=dashboard');
    expect(asked).toBe(1);
  });

  it('says nothing for an empty inbox and never asks without MANAGE_CASES', async () => {
    let asked = 0;
    server.use(
      http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/dashboard/inbox/count', () => {
        asked += 1;
        return HttpResponse.json({ groups: 0, reports: 0, oldest_seconds: null });
      }),
    );
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    openAdmin('review');
    await screen.findByRole('tab', { name: 'Packages' }, SLOW);
    expect(asked).toBe(0);
    expect(screen.queryByText(/waiting for someone/)).not.toBeInTheDocument();
  });
});
