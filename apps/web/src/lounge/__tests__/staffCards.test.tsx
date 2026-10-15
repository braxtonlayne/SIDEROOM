// Buttons on system-user cards (ADM-STF-004 offers in a DM, ADM-STF-007 staff cards).
import { ToastProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { ComponentRows } from '../msg/Components';
import { parseStaffButton } from '../msg/staffActions';

const offerRows = [
  {
    type: 1,
    components: [
      { type: 2, style: 3, label: 'Accept', custom_id: 'sr:offer-accept:77' },
      { type: 2, style: 4, label: 'Decline', custom_id: 'sr:offer-decline:77' },
    ],
  },
];

const view = (rows: typeof offerRows) =>
  render(
    <ToastProvider>
      <ComponentRows rows={rows} channelId="500" messageId="900" />
    </ToastProvider>,
  );

describe('parseStaffButton', () => {
  it('tells offers from card actions and ignores app buttons', () => {
    expect(parseStaffButton('sr:offer-accept:77')).toEqual({
      kind: 'offer',
      offerId: '77',
      accept: true,
    });
    expect(parseStaffButton('sr:offer-decline:5')).toEqual({
      kind: 'offer',
      offerId: '5',
      accept: false,
    });
    expect(parseStaffButton('sr:ack:12')).toEqual({ kind: 'card', customId: 'sr:ack:12' });
    expect(parseStaffButton('some-app-button')).toBeNull();
    expect(parseStaffButton(undefined)).toBeNull();
  });
});

describe('staff card buttons', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
  });

  it('answers an appointment offer and then disables both buttons', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/staff/offers/77/respond', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    view(offerRows);
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(body).toEqual({ accept: true }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Decline' })).toBeDisabled());
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
  });

  it('declines through the same endpoint', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/staff/offers/77/respond', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    view(offerRows);
    await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
    await waitFor(() => expect(body).toEqual({ accept: false }));
  });

  it('runs a staff card action with the message coordinates and shows the server text', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/admin/card-actions', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ ok: true, message: 'Alert acknowledged.' });
      }),
    );
    view([
      { type: 1, components: [{ type: 2, style: 2, label: 'Acknowledge', custom_id: 'sr:ack:3' }] },
    ]);
    await userEvent.click(screen.getByRole('button', { name: 'Acknowledge' }));
    await waitFor(() =>
      expect(body).toEqual({ channel_id: '500', message_id: '900', custom_id: 'sr:ack:3' }),
    );
    expect(await screen.findByText('Alert acknowledged.')).toBeInTheDocument();
  });

  it('keeps the buttons usable after a refusal', async () => {
    server.use(
      http.post('*/api/v1/staff/offers/77/respond', () =>
        HttpResponse.json({ code: 900001, message: 'The offer has expired.' }, { status: 409 }),
      ),
    );
    view(offerRows);
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByText('The offer has expired.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
  });
});
