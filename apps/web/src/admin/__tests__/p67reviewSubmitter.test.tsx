import { switchStudySurfacesOn } from '../../study/testing';
// SEC-AUD-208 (D-691, REV-RV-001): press a submitter in the review queue to see only their submissions.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { submission } from '../../review/__tests__/fixtures';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

function useQueue(pageSize = 25) {
  const seen: string[] = [];
  server.use(
    http.get('*/api/v1/admin/review/queue', ({ request }) => {
      const u = new URL(request.url);
      seen.push(u.search);
      const offset = Number(u.searchParams.get('offset') ?? 0);
      const only = u.searchParams.get('submitter');
      const items = Array.from({ length: only ? 2 : pageSize }, (_, i) =>
        submission(String(offset + i + 1), {
          title: `S${offset + i + 1}`,
          submitter_id: only ?? (i % 2 ? '88' : '77'),
        }),
      );
      return HttpResponse.json({ items, total: only ? 2 : 60, limit: 25, offset });
    }),
  );
  return seen;
}

describe('review queue submitter filter', () => {
  beforeEach(() => {
    switchStudySurfacesOn();
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING'] });
  });

  it('pressing a submitter requests with submitter, shows a chip that clears, carries the URL and resets paging', async () => {
    const seen = useQueue();
    const user = userEvent.setup();
    const { router } = openAdmin('review');
    await screen.findByRole('table', { name: 'Review queue' });
    expect(seen[0]).not.toContain('submitter=');

    await user.click(screen.getByRole('button', { name: 'Next' }));
    await waitFor(() => expect(seen.at(-1)).toContain('offset=25'));

    await screen.findByText('S26');
    const press = within(screen.getByRole('table', { name: 'Review queue' })).getAllByRole(
      'button',
      {
        name: /^88\. Show only this person.s submissions$/,
      },
    )[0]!;
    await user.click(press);
    await waitFor(() => expect(seen.at(-1), JSON.stringify(seen)).toContain('submitter=88'));
    expect(seen.at(-1)).toContain('offset=0');
    expect(router.state.location.searchStr).toContain('submitter=88');
    const chip = await screen.findByRole('button', { name: /Submitted by Account 88/ });

    await user.click(chip);
    await waitFor(() => expect(seen.at(-1)).not.toContain('submitter='));
    expect(router.state.location.searchStr).not.toContain('submitter');
    expect(screen.queryByRole('button', { name: /Submitted by Account/ })).toBeNull();
  });

  it('opens already filtered from the address', async () => {
    const seen = useQueue();
    openAdmin('review?submitter=77'); // the hub opens on the Submissions tab, where the filter lives
    expect(
      await screen.findByRole('button', { name: /Submitted by Account 77/ }),
    ).toBeInTheDocument();
    expect(seen[0]).toContain('submitter=77');
  });
});
