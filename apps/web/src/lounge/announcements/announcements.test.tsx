// Platform announcement banner (ADM-PLT-009 client side).
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { AnnouncementBanner } from './index';

const note = (over: Record<string, unknown> = {}) => ({
  id: '1',
  title: 'Maintenance tonight',
  body: 'Short downtime at 22:00 UTC.',
  severity: 'warning',
  starts_at_ms: 1,
  ...over,
});

describe('AnnouncementBanner', () => {
  beforeEach(() => localStorage.clear());

  it('shows active notices and renders nothing when there are none', async () => {
    server.use(http.get('*/api/v1/announcements/active', () => HttpResponse.json([note()])));
    const first = render(<AnnouncementBanner />);
    expect(await screen.findByText('Maintenance tonight')).toBeInTheDocument();
    first.unmount();

    server.use(http.get('*/api/v1/announcements/active', () => HttpResponse.json([])));
    const second = render(<AnnouncementBanner />);
    await waitFor(() => expect(second.container).toBeEmptyDOMElement());
  });

  it('remembers a dismissed notice but never lets a critical one go', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () =>
        HttpResponse.json([
          note(),
          note({ id: '2', title: 'Outage', severity: 'critical', body: 'Down.' }),
        ]),
      ),
    );
    const view = render(<AnnouncementBanner />);
    await screen.findByText('Maintenance tonight');
    expect(screen.getAllByRole('button', { name: 'Dismiss' })).toHaveLength(1);
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText('Maintenance tonight')).not.toBeInTheDocument();
    expect(screen.getByText('Outage')).toBeInTheDocument();

    view.unmount();
    render(<AnnouncementBanner />);
    await screen.findByText('Outage');
    expect(screen.queryByText('Maintenance tonight')).not.toBeInTheDocument();
  });

  it('keeps quiet when the request fails', async () => {
    server.use(
      http.get('*/api/v1/announcements/active', () => new HttpResponse(null, { status: 500 })),
    );
    const { container } = render(<AnnouncementBanner />);
    await new Promise((r) => setTimeout(r, 30));
    expect(container).toBeEmptyDOMElement();
  });
});
