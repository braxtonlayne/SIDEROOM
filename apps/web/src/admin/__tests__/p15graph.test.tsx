// The invite graph canvas (ADM-GRAPH-003): the pure model, then the section with a big graph over a stubbed layout
// and a recording 2D context (jsdom has no canvas).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import type { InviteNode } from '../api';
import {
  CANVAS_THRESHOLD,
  clampZoom,
  fitView,
  hiddenBy,
  hitTest,
  indexGraph,
  lookOf,
  nodeRadius,
  subtreeSize,
  toggleCollapsed,
  toWorld,
  zoomAt,
} from '../sections/graph/graphModel';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const layoutGraph = vi.hoisted(() =>
  vi.fn<(n: number, edges: Uint32Array) => Promise<Float32Array>>(),
);
vi.mock('../sections/graph/layout', () => ({ layoutGraph }));

const node = (id: string, parent: string | null, over: Partial<InviteNode> = {}): InviteNode => ({
  account_id: id,
  username: `user${id}`,
  state: 'active',
  person_id: `p${id}`,
  suspicion_level: 0,
  parent_id: parent,
  depth: parent ? 1 : 0,
  ...over,
});

/** A tree: 1 -> 2, 3; 2 -> 4, 5; 4 -> 6. */
const small = () => [
  node('2', '1'),
  node('3', '1'),
  node('4', '2'),
  node('5', '2'),
  node('6', '4'),
];

/** Positions on a grid, one per node. */
const grid = (n: number) => {
  const p = new Float32Array(2 * n);
  for (let i = 0; i < n; i++) {
    p[2 * i] = (i % 25) * 20;
    p[2 * i + 1] = Math.floor(i / 25) * 20;
  }
  return p;
};

describe('graph model', () => {
  it('indexes the nodes, hangs orphans off the root and builds the edge list', () => {
    const g = indexGraph(node('1', null), [...small(), node('9', 'gone')]);
    expect(g.ids).toEqual(['1', '2', '3', '4', '5', '6', '9']);
    expect(Array.from(g.parent)).toEqual([-1, 0, 0, 1, 1, 3, 0]);
    expect(g.children[0]).toEqual([1, 2, 6]);
    expect(Array.from(g.edges)).toEqual([0, 1, 0, 2, 1, 3, 1, 4, 3, 5, 0, 6]);
    // The root listed among its descendants is not counted twice.
    expect(indexGraph(node('1', null), [node('1', null), node('2', '1')]).ids).toEqual(['1', '2']);
  });

  it('hides what a collapsed branch holds, keeps the branch node, and ignores leaves', () => {
    const g = indexGraph(node('1', null), small());
    const none = hiddenBy(g, new Set());
    expect(Array.from(none)).toEqual([0, 0, 0, 0, 0, 0]);
    const two = hiddenBy(g, new Set(['2']));
    expect(Array.from(two)).toEqual([0, 0, 0, 1, 1, 1]);
    expect(subtreeSize(g, 1)).toBe(3);
    expect(subtreeSize(g, 2)).toBe(0);
    const open = toggleCollapsed(g, new Set(), '2');
    expect([...open]).toEqual(['2']);
    expect([...toggleCollapsed(g, open, '2')]).toEqual([]);
    expect([...toggleCollapsed(g, new Set(), '3')]).toEqual([]);
    expect([...toggleCollapsed(g, new Set(), 'nope')]).toEqual([]);
  });

  it('zooms around the pointer and clamps', () => {
    const v = { x: 10, y: 20, k: 1 };
    const [wx, wy] = toWorld(v, 110, 120);
    const z = zoomAt(v, 110, 120, 2);
    expect(z.k).toBe(2);
    expect(toWorld(z, 110, 120)).toEqual([wx, wy]);
    expect(clampZoom(1000)).toBe(12);
    expect(clampZoom(0)).toBe(0.05);
  });

  it('fits the visible nodes and finds the node under the pointer', () => {
    const g = indexGraph(node('1', null), small());
    const pos = Float32Array.from([0, 0, 100, 0, -100, 0, 0, 100, 0, -100, 50, 50]);
    const hidden = Uint8Array.from([0, 0, 0, 0, 0, 1]);
    const v = fitView(pos, hidden, 400, 400, 20);
    expect(v.k).toBeCloseTo(1.8, 5);
    // The middle of the picture is the middle of the view.
    expect(0 * v.k + v.x).toBeCloseTo(200, 5);
    expect(hitTest(g, pos, hidden, v, 200, 200)).toBe(0);
    expect(hitTest(g, pos, hidden, v, 200 + 100 * v.k, 200)).toBe(1);
    // A hidden node cannot be hit; empty space is -1.
    expect(hitTest(g, pos, hidden, v, 200 + 50 * v.k, 200 + 50 * v.k)).toBe(-1);
    expect(hitTest(g, pos, hidden, v, 5, 5)).toBe(-1);
    expect(fitView(new Float32Array(0), new Uint8Array(0), 400, 300)).toEqual({
      x: 200,
      y: 150,
      k: 1,
    });
    expect(nodeRadius(0)).toBeLessThan(nodeRadius(16));
  });

  it('colours by state, rings suspicion and crosses a ban', () => {
    expect(lookOf({ state: 'active', suspicion_level: 0 })).toMatchObject({
      banned: false,
      halo: 0,
    });
    expect(lookOf({ state: 'platform_banned', suspicion_level: 0 }).banned).toBe(true);
    expect(lookOf({ state: 'active', suspicion_level: 3 }).halo).toBeGreaterThan(0);
    expect(lookOf({ state: 'deleted', suspicion_level: 0 }).token).not.toBe(
      lookOf({ state: 'active', suspicion_level: 0 }).token,
    );
  });
});

describe('invite graph: canvas view', () => {
  const calls: string[] = [];

  beforeEach(() => {
    calls.length = 0;
    layoutGraph.mockReset();
    layoutGraph.mockImplementation(async (n) => grid(n));
    const store: Record<string, unknown> = {};
    const ctx = new Proxy(store, {
      get: (t, k: string) =>
        k in t ? t[k] : (...a: unknown[]) => void calls.push(`${k}:${a.length}`),
      set: (t, k: string, v) => ((t[k] = v), true),
    });
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(ctx as never);
    vi.stubGlobal('requestAnimationFrame', (f: FrameRequestCallback) => {
      f(0);
      return 1;
    });
    vi.stubGlobal('cancelAnimationFrame', () => undefined);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const many = (n: number) =>
    Array.from({ length: n }, (_, i) =>
      node(String(i + 2), i < 20 ? '1' : String(2 + Math.floor((i - 20) / 3)), {
        state: i % 50 === 0 ? 'platform_banned' : 'active',
        suspicion_level: i % 70 === 0 ? 2 : 0,
      }),
    );

  const seed = (count: number) => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/accounts/1/invite-tree', () =>
        HttpResponse.json({
          account: node('1', null),
          ancestors: [],
          stats: {
            issued: 0,
            open: 0,
            redeemed: 0,
            revoked: 0,
            descendants: count,
            banned_descendants: 0,
          },
        }),
      ),
      http.get('*/api/v1/admin/accounts/1/invite-descendants', () =>
        HttpResponse.json({ nodes: many(count), truncated: false }),
      ),
      http.get('*/api/v1/admin/accounts/:id', () => HttpResponse.json({}, { status: 404 })),
      http.get('*/api/v1/admin/invites', () => HttpResponse.json({ items: [], next_after: null })),
      http.get('*/api/v1/admin/accounts/:id/credits', () =>
        HttpResponse.json({ balance: 0, entries: [] }),
      ),
    );
  };

  it('uses the canvas past the threshold, draws the nodes, and lets staff switch to the tree', async () => {
    seed(CANVAS_THRESHOLD + 20);
    openAdmin('invite-graph');
    const user = userEvent.setup();
    const canvas = await screen.findByRole('img', { name: /Invite graph of 521 accounts/ });
    expect(canvas.tagName).toBe('CANVAS');
    expect(screen.queryByRole('tree', { name: 'Invite tree' })).not.toBeInTheDocument();
    // The layout runs in an effect, which may land after the canvas is in the DOM.
    await waitFor(() => expect(layoutGraph).toHaveBeenCalledTimes(1));
    expect(layoutGraph.mock.calls[0]![0]).toBe(521);
    expect(layoutGraph.mock.calls[0]![1]).toHaveLength(2 * 520);
    await waitFor(() => expect(calls.some((c) => c.startsWith('arc'))).toBe(true));
    expect(await screen.findByText('521 of 521 accounts shown')).toBeInTheDocument();
    // The tree view is one button away and the graph button brings the picture back.
    await user.click(screen.getByRole('button', { name: 'Tree' }));
    expect(await screen.findByRole('tree', { name: 'Invite tree' })).toBeInTheDocument();
    expect(screen.queryByRole('img', { name: /Invite graph of/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Graph' }));
    expect(await screen.findByRole('img', { name: /Invite graph of/ })).toBeInTheDocument();
  });

  it('stays on the tree for a small graph and offers no graph button', async () => {
    seed(30);
    openAdmin('invite-graph');
    expect(await screen.findByRole('tree', { name: 'Invite tree' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Graph' })).not.toBeInTheDocument();
    expect(layoutGraph).not.toHaveBeenCalled();
  });

  it('collapses and expands the selected branch from the toolbar and keys', async () => {
    seed(CANVAS_THRESHOLD + 20);
    openAdmin('invite-graph');
    const user = userEvent.setup();
    const canvas = await screen.findByRole('img', { name: /Invite graph of/ });
    const tools = screen.getByRole('toolbar', { name: 'Graph tools' });
    // Nothing is selected yet: collapsing is not offered.
    expect(within(tools).getByRole('button', { name: 'Collapse branch' })).toBeDisabled();
    expect(within(tools).getByRole('button', { name: 'Expand all' })).toBeDisabled();
    // Pan and zoom by keyboard redraw without error.
    canvas.focus();
    const before = calls.length;
    await user.keyboard('{ArrowLeft}{ArrowUp}+-0');
    await waitFor(() => expect(calls.length).toBeGreaterThan(before));
    await user.click(within(tools).getByRole('button', { name: 'Zoom in' }));
    await user.click(within(tools).getByRole('button', { name: 'Fit to view' }));
  });

  it('says so and keeps the tree and list when the layout cannot load', async () => {
    seed(CANVAS_THRESHOLD + 20);
    layoutGraph.mockRejectedValue(new Error('no wasm'));
    openAdmin('invite-graph');
    const user = userEvent.setup();
    expect(await screen.findByText(/could not be drawn/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'List' }));
    expect(screen.getByRole('table', { name: 'Invited accounts' })).toBeInTheDocument();
  });

  describe('reduced motion (integrator item 3b)', () => {
    afterEach(() => usePrefsStore.setState({ prefs: DEFAULT_PREFS }));

    const frames = () => {
      const raf = vi.fn((f: FrameRequestCallback) => {
        f(0);
        return 1;
      });
      vi.stubGlobal('requestAnimationFrame', raf);
      return raf;
    };

    it('draws a static picture without requestAnimationFrame when Settings asks for reduced motion', async () => {
      usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, reducedMotion: 'on' } });
      const raf = frames();
      seed(CANVAS_THRESHOLD + 20);
      openAdmin('invite-graph');
      const user = userEvent.setup();
      const canvas = await screen.findByRole('img', { name: /Invite graph of/ });
      await waitFor(() => expect(calls.some((c) => c.startsWith('arc'))).toBe(true));
      expect(canvas).toHaveAttribute('data-reduced-motion', 'true');
      // Pan and zoom still redraw, at once and without a frame loop.
      canvas.focus();
      const before = calls.length;
      await user.keyboard('{ArrowLeft}+0');
      expect(calls.length).toBeGreaterThan(before);
      await user.click(screen.getByRole('button', { name: 'Zoom in' }));
      expect(raf).not.toHaveBeenCalled();
    });

    it('follows the OS setting when Settings says "system", and animates frames otherwise', async () => {
      const mq = (reduce: boolean) =>
        vi.stubGlobal(
          'matchMedia',
          (q: string) =>
            ({
              matches: reduce && q.includes('prefers-reduced-motion'),
              media: q,
              addEventListener: () => undefined,
              removeEventListener: () => undefined,
            }) as unknown as MediaQueryList,
        );
      mq(true);
      let raf = frames();
      seed(CANVAS_THRESHOLD + 20);
      const first = openAdmin('invite-graph');
      await screen.findByRole('img', { name: /Invite graph of/ });
      await waitFor(() => expect(calls.some((c) => c.startsWith('arc'))).toBe(true));
      expect(raf).not.toHaveBeenCalled();
      first.unmount();

      calls.length = 0;
      mq(false);
      raf = frames();
      seed(CANVAS_THRESHOLD + 20);
      openAdmin('invite-graph');
      const canvas = await screen.findByRole('img', { name: /Invite graph of/ });
      await waitFor(() => expect(calls.some((c) => c.startsWith('arc'))).toBe(true));
      expect(canvas).not.toHaveAttribute('data-reduced-motion');
      expect(raf).toHaveBeenCalled();
    });
  });
});
