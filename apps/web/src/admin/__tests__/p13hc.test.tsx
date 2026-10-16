// H-30 (D-235): a webhook message is listed under its creator with a "via webhook" marker.
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

describe('Private content: webhook messages (D-235)', () => {
  it('shows "via webhook <name>" beside the creator', async () => {
    asStaff({ perms: ['READ_PRIVATE_CONTENT'] });
    server.use(
      http.get('*/api/v1/admin/private/channels/55/messages', () =>
        HttpResponse.json({
          channel: { id: '55', kind: 0, private: true, recipients: [] },
          deleted_retention_days: 30,
          next_before: null,
          items: [
            {
              id: '3',
              author_id: '2',
              author_name: 'bob',
              content: 'deploy done',
              attachment_count: 0,
              created_at_ms: 1_700_000_000_000,
              edits: [],
              visibility: 'visible',
              webhook: { id: '4242', name: 'Deploys' },
            },
            {
              id: '2',
              author_id: '2',
              author_name: 'bob',
              content: 'plain',
              attachment_count: 0,
              created_at_ms: 1_699_999_000_000,
              edits: [],
              visibility: 'visible',
            },
          ],
        }),
      ),
    );
    openAdmin('private-content');
    await userEvent.type(await screen.findByLabelText('Channel ID'), '55');
    await userEvent.type(screen.getByLabelText(/Reason/), 'Report 1');
    await userEvent.click(screen.getByRole('button', { name: 'Read messages' }));
    expect(await screen.findByText('deploy done')).toBeInTheDocument();
    expect(screen.getAllByText('via webhook Deploys')).toHaveLength(1);
  });
});
