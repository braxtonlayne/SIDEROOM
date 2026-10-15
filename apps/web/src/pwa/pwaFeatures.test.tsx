import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../router';
import { drafts } from '../lounge/drafts';
import { keyboardHeight } from '../lounge/useVisualViewport';
import { resetPrefs, signedIn } from '../settings/user/__tests__/helpers';
import { resetStore, useStore } from '../store';
import { http, HttpResponse, server } from '../test/server';
import { WebPushCard } from './WebPushCard';
import { disablePush, enablePush, pushStatus, urlBase64ToBytes } from './push';
import { queuedFiles } from '../lounge/queuedFiles';
import { parseShare, shareToDraft } from './shareText';
import { badgeCount, useAppBadge } from './useAppBadge';

const sharedFile = new File(['x'], 'pic.png', { type: 'image/png' });
vi.mock('./shareFiles', () => ({
  readSharedFiles: () => Promise.resolve([sharedFile]),
  clearSharedFiles: () => Promise.resolve(),
}));

const g = globalThis as unknown as Record<string, unknown>;

describe('XC-PWA-005 app badge', () => {
  beforeEach(() => {
    resetPrefs();
    resetStore();
  });
  afterEach(() => {
    delete g.setAppBadge;
    delete (navigator as unknown as Record<string, unknown>).setAppBadge;
    delete (navigator as unknown as Record<string, unknown>).clearAppBadge;
  });

  it('sums mentions across channels', () => {
    expect(
      badgeCount({
        a: { channel_id: 'a', last_message_id: null, mention_count: 2 },
        b: { channel_id: 'b', last_message_id: null, mention_count: 3 },
      }),
    ).toBe(5);
  });

  it('follows the Lounge read states and clears when there are none', () => {
    const set = vi.fn(() => Promise.resolve());
    const clear = vi.fn(() => Promise.resolve());
    Object.assign(navigator, { setAppBadge: set, clearAppBadge: clear });
    renderHook(() => useAppBadge());
    expect(clear).toHaveBeenCalled();
    act(() => {
      useStore.setState({
        readStates: { a: { channel_id: 'a', last_message_id: null, mention_count: 4 } },
      });
    });
    expect(set).toHaveBeenLastCalledWith(4);
  });
});

describe('XC-PWA-004 web push', () => {
  beforeEach(() => {
    localStorage.clear();
    signedIn();
  });

  it('reports an unsupported browser without throwing', async () => {
    expect(await pushStatus()).toEqual({ kind: 'unsupported', reason: 'no-browser-support' });
    expect(await enablePush('x')).toEqual({ kind: 'unsupported', reason: 'no-browser-support' });
    expect(await disablePush()).toEqual({ kind: 'unsupported', reason: 'no-browser-support' });
  });

  it('shows the unsupported notice in settings', async () => {
    render(<WebPushCard />);
    expect(await screen.findByText(/does not support push notifications/)).toBeInTheDocument();
  });

  it('says the server has no push when the key route is missing', async () => {
    const pm = { getSubscription: () => Promise.resolve(null) };
    Object.assign(navigator, {
      serviceWorker: {
        getRegistration: () => Promise.resolve({ pushManager: pm }),
        ready: Promise.resolve({ pushManager: pm }),
      },
    });
    Object.assign(window, { PushManager: class {} });
    g.Notification = Object.assign(class {}, {
      permission: 'default',
      requestPermission: () => Promise.resolve('granted'),
    });
    server.use(
      http.get('*/api/v1/push/vapid-key', () => HttpResponse.json({ code: 0 }, { status: 404 })),
    );
    try {
      expect(await pushStatus()).toEqual({ kind: 'off', permission: 'default' });
      expect(await enablePush('x')).toEqual({ kind: 'unsupported', reason: 'no-server-support' });
    } finally {
      delete (navigator as unknown as Record<string, unknown>).serviceWorker;
      delete (window as unknown as Record<string, unknown>).PushManager;
      delete g.Notification;
    }
  });

  it('decodes a VAPID key', () => {
    expect(Array.from(urlBase64ToBytes('AQID'))).toEqual([1, 2, 3]);
  });
});

describe('XC-PWA-006 share target', () => {
  it('builds a draft from title, text and url', () => {
    expect(parseShare('?title=T&text=Look&url=https%3A%2F%2Fa.b')).toEqual({
      title: 'T',
      text: 'Look',
      url: 'https://a.b',
      hasFiles: false,
    });
    expect(parseShare('?files=1').hasFiles).toBe(true);
    expect(shareToDraft({ title: 'T', text: 'Look', url: 'https://a.b' })).toBe(
      'T\nLook\nhttps://a.b',
    );
    expect(shareToDraft({ title: '', text: 'see https://a.b', url: 'https://a.b' })).toBe(
      'see https://a.b',
    );
  });

  it('lists channels, puts the text in the chosen draft and opens it', async () => {
    resetPrefs();
    drafts.clear();
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([{ id: '5', name: 'Club' }])),
      http.get('*/api/v1/guilds/5/channels', () =>
        HttpResponse.json([
          { id: '50', type: 0, name: 'general', guild_id: '5' },
          { id: '51', type: 2, name: 'voice', guild_id: '5' },
        ]),
      ),
      http.get('*/api/v1/users/@me/channels', () => HttpResponse.json([])),
    );
    const router = createAppRouter(
      createMemoryHistory({ initialEntries: ['/share?text=Hello%20there'] }),
    );
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole('button', { name: '#general' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '#voice' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: '#general' }));
    expect(drafts.get('50')).toBe('Hello there');
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/5/50'));
  });
});

describe('G-77 share target without a service worker', () => {
  it('says to open SIDE ROOM once and share again when nothing arrived', async () => {
    resetPrefs();
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/channels', () => HttpResponse.json([])),
    );
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/share'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByText(/Nothing was shared/)).toBeInTheDocument();
    expect(
      screen.getByText('Sharing from another app? Open SIDE ROOM once, then share again.'),
    ).toBeInTheDocument();
  });
});

describe('XC-PWA-007 keyboard height', () => {
  it('only reacts when the keyboard takes real space', () => {
    expect(keyboardHeight(800, 800)).toBeNull();
    expect(keyboardHeight(760, 800)).toBeNull();
    expect(keyboardHeight(480, 800)).toBe(480);
  });

  it('G-18 queues shared files for the chosen channel without a draft', async () => {
    resetPrefs();
    drafts.clear();
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([{ id: '5', name: 'Club' }])),
      http.get('*/api/v1/guilds/5/channels', () =>
        HttpResponse.json([{ id: '50', type: 0, name: 'general', guild_id: '5' }]),
      ),
      http.get('*/api/v1/users/@me/channels', () => HttpResponse.json([])),
    );
    const setSpy = vi.spyOn(queuedFiles, 'set');
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/share?files=1'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByText('1 file attached')).toBeInTheDocument();
    await userEvent.click(await screen.findByRole('button', { name: '#general' }));
    expect(drafts.get('50')).toBeUndefined();
    expect(setSpy).toHaveBeenCalledWith('50', [sharedFile]);
  });
});
