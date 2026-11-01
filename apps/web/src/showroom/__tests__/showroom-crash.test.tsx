// OPS-CRASH-001 x UX-SHOW-001 (wave 45 integration): the Showroom and the crash telemetry side by side. Fixture
// interactions never become crash reports or ticks; a real crash while the preview is on still reports, with the route
// NAME only and nothing from the fixtures.
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { currentRouteName, installCrashTelemetry } from '../../crash';
import { resetCrashState } from '../../crash/report';
import { ticks } from '../../crash/ringBuffer';
import { setClientFlags } from '../../moderation/reports';
import { createAppRouter } from '../../router';
import { useStore } from '../../store/store';
import { http, HttpResponse, server } from '../../test/server';
import { FIXTURE_GAMES, FIXTURE_SERVERS, FIXTURE_SHELVES } from '../fixtures';
import { SHOWROOM_FLAG } from '../flag';
import { ShowroomBanner } from '../ShowroomBanner';
import { setShowroom, useShowroom } from '../state';

/** Every string in the fixtures that could identify them (names, titles, handles, notes, tags). */
function fixtureNeedles(): string[] {
  const out = new Set<string>();
  const walk = (v: unknown) => {
    if (typeof v === 'string') {
      if (v.length >= 5 && !/^\d{4}-\d{2}/.test(v) && !/^fx-/.test(v)) out.add(v);
    } else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === 'object') Object.values(v).forEach(walk);
  };
  walk([FIXTURE_SERVERS, FIXTURE_SHELVES, FIXTURE_GAMES]);
  return [...out];
}

let posted: string[] = [];
let uninstall: (() => void) | undefined;

beforeEach(() => {
  posted = [];
  localStorage.clear();
  resetCrashState();
  ticks.clear();
  setClientFlags({});
  useShowroom.setState({ on: false, hint: null });
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' } },
    guilds: { '70': { id: '70', name: 'Club' } },
  } as never);
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/shelves/visible', () => HttpResponse.json({ shelves: [] })),
    http.post('*/api/v1/crash-reports', async ({ request }) => {
      posted.push(await request.text());
      return HttpResponse.json({ id: '1', count: 1 }, { status: 202 });
    }),
  );
});

afterEach(() => {
  uninstall?.();
  uninstall = undefined;
  resetCrashState();
  ticks.clear();
  setClientFlags({});
  useShowroom.setState({ on: false, hint: null });
});

function open(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  uninstall = installCrashTelemetry({ routeName: () => currentRouteName(router) });
  render(
    <>
      <RouterProvider router={router} />
      <ShowroomBanner />
    </>,
  );
  return router;
}

describe('the Showroom and the crash telemetry', () => {
  it('fixture interactions never produce a crash report or a tick', async () => {
    const user = userEvent.setup();
    act(() => {
      setClientFlags({ [SHOWROOM_FLAG]: true });
      setShowroom(true);
    });
    open('/games');
    const page = await screen.findByTestId('showroom-arcade');
    for (const b of within(page).getAllByRole('button', { name: /Play|Notify me/ }))
      await user.click(b);
    expect(screen.getByTestId('showroom-hint')).toBeInTheDocument();
    // Give a lazily loaded report builder every chance to run.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(posted).toEqual([]);
    expect(ticks.snapshot()).toEqual([]);
  });

  it('a crash while the preview is on still reports, with the route name and no fixture content', async () => {
    act(() => {
      setClientFlags({ [SHOWROOM_FLAG]: true });
      setShowroom(true);
    });
    open('/games');
    await screen.findByTestId('showroom-arcade');

    // A hostile crash: the message, the stack text and a frame URL all carry fixture content.
    const needles = fixtureNeedles();
    expect(needles.length).toBeGreaterThan(20);
    const err = new Error(`boom in ${FIXTURE_SERVERS[0]!.name} / ${FIXTURE_GAMES[0]!.title}`);
    err.stack = [
      `Error: ${needles.slice(0, 5).join(' ')}`,
      `    at renderSample (https://app.test/assets/PreviewArcade-Ab12Cd34.js?item=${encodeURIComponent(
        needles[1]!,
      )}#${encodeURIComponent(needles[2]!)}:10:5)`,
      '    at https://app.test/assets/index-Bx3k9Qa1.js:120:15',
    ].join('\n');
    window.dispatchEvent(new ErrorEvent('error', { error: err, message: err.message }));

    await vi.waitFor(() => expect(posted).toHaveLength(1));
    const body = posted[0]!;
    const report = JSON.parse(body) as { route: string; kind: string };
    expect(report.kind).toBe('error');
    expect(report.route).toBe('games');
    for (const n of needles) expect(body, n).not.toContain(n);
    for (const n of needles) expect(body, n).not.toContain(encodeURIComponent(n));
    expect(body).not.toMatch(/demo_|\(sample\)/);
  });
});
