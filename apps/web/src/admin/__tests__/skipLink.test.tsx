import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

/** RDY-A11Y-002: the Admin shell has a skip link to its main region. */
describe('admin skip link', () => {
  it('moves focus to the main region', async () => {
    asStaff({ perms: [], owner: true });
    server.use(http.get('*/api/v1/admin/stats', () => HttpResponse.json({})));
    const user = userEvent.setup();
    openAdmin('dashboard');
    const skip = await screen.findByRole('link', { name: 'Skip to main content' });
    await user.click(skip);
    expect(screen.getByRole('main')).toHaveFocus();
  });
});
