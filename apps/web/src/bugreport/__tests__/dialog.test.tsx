import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse, server } from '../../test/server';
import { BugReportDialog } from '../BugReportDialog';
import type { BugReportStart } from '../openState';

vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useRouter: () => ({ state: { matches: [{ routeId: '/lounge/$guildId/$channelId' }] } }),
}));
vi.mock('../encode', async (orig) => ({
  ...(await orig<typeof import('../encode')>()),
  renderJpeg: vi.fn(async () => ({
    blob: new Blob([new Uint8Array([255, 216, 255, 217])], { type: 'image/jpeg' }),
    width: 200,
    height: 100,
  })),
}));

beforeEach(() => {
  // jsdom has no 2d canvas; the preview then stays blank.
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
});
afterEach(() => vi.restoreAllMocks());

const frameStart = (rects = [{ x: 10, y: 10, w: 50, h: 20 }]): BugReportStart => ({
  frame: {
    source: document.createElement('canvas'),
    w: window.innerWidth * 2,
    h: window.innerHeight * 2,
  },
  rects,
  viewport: { w: window.innerWidth, h: window.innerHeight },
});

function capture() {
  let sent: Record<string, unknown> | undefined;
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.post('*/api/v1/bug-reports', async ({ request }) => {
      sent = (await request.json()) as Record<string, unknown>;
      return HttpResponse.json({ id: '42' }, { status: 201 });
    }),
  );
  return () => sent;
}

async function ready(user: ReturnType<typeof userEvent.setup>) {
  const dialog = await screen.findByRole('dialog', { name: 'Report a bug' });
  await user.click(within(dialog).getByText('What is sent'));
  // The bundle is built once the service worker state has been read.
  await vi.waitFor(() =>
    expect(within(dialog).getByLabelText(/debug information/).textContent).toContain('"commit"'),
  );
  return dialog;
}

describe('BugReportDialog (D-647..D-650)', () => {
  it('cannot be sent without a description, and sends exactly the JSON it shows', async () => {
    const sent = capture();
    const user = userEvent.setup();
    render(<BugReportDialog start={null} onClose={() => undefined} />);
    const dialog = await ready(user);
    const send = within(dialog).getByRole('button', { name: 'Send' });
    expect(send).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/What went wrong/), '  The list stays empty  ');
    expect(within(dialog).getByText('24 / 2000')).toBeInTheDocument();
    expect(send).toBeEnabled();
    const shown = JSON.parse(within(dialog).getByLabelText(/debug information/).textContent ?? '');
    await user.click(send);
    expect(await screen.findByText('Thank you. Report #42 was sent.')).toBeInTheDocument();
    const body = sent() as { description: string; bundle: unknown; screenshot?: unknown };
    expect(body.description).toBe('The list stays empty');
    expect(body.bundle).toEqual(shown);
    expect('screenshot' in body).toBe(false);
  });

  it('starts with the page covers painted in, and sends none of it when the screenshot is removed', async () => {
    const sent = capture();
    const user = userEvent.setup();
    render(<BugReportDialog start={frameStart()} onClose={() => undefined} />);
    const dialog = await ready(user);
    expect(within(dialog).getByRole('button', { name: 'Remove black box 1' })).toBeInTheDocument();
    expect(within(dialog).getByText(/already blacked out/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Remove screenshot' }));
    expect(within(dialog).queryByRole('button', { name: /Remove black box/ })).toBeNull();
    await user.type(within(dialog).getByLabelText(/What went wrong/), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Send' }));
    await screen.findByText(/was sent/);
    expect('screenshot' in (sent() ?? {})).toBe(false);
  });

  it('sends the picture with the number of boxes, and warns when nothing was covered automatically', async () => {
    const sent = capture();
    const user = userEvent.setup();
    render(<BugReportDialog start={frameStart([])} onClose={() => undefined} />);
    const dialog = await ready(user);
    expect(
      within(dialog).getByText('Check the picture: black out anything private before you send it.'),
    ).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText(/What went wrong/), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Send' }));
    await screen.findByText(/was sent/);
    expect(sent()?.screenshot).toMatchObject({ type: 'image/jpeg', masked: 0 });
  });

  it('says in words that the member is over the limit', async () => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.post('*/api/v1/bug-reports', () =>
        HttpResponse.json({ message: 'slow down' }, { status: 429 }),
      ),
    );
    const user = userEvent.setup();
    render(<BugReportDialog start={null} onClose={() => undefined} />);
    const dialog = await ready(user);
    await user.type(within(dialog).getByLabelText(/What went wrong/), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/several reports lately/);
  });

  it('tells who reads the report and how long it is kept', async () => {
    render(<BugReportDialog start={null} onClose={() => undefined} />);
    expect(
      await screen.findByText(
        /Staff of this SIDE ROOM can read this report\. The screenshot is deleted when the report is closed or after 30 days, the report after 90 days\./,
      ),
    ).toBeInTheDocument();
  });
});
