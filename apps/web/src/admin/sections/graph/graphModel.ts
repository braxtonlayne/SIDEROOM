// The model behind the invite graph canvas (ADM-GRAPH-003): index the nodes, work out what a collapsed subtree
// hides, convert between screen and world coordinates and find the node under the pointer. Pure functions, no
// DOM, so they are tested without a canvas. The layout itself comes from `sr-graph-wasm`.
import type { InviteNode } from '../../api';

/** Above this many nodes the canvas is the default view; below it the tree and list views are enough (G-24). */
export const CANVAS_THRESHOLD = 500;

export interface GraphIndex {
  /** Node 0 is the root. */
  ids: string[];
  nodes: InviteNode[];
  index: Map<string, number>;
  /** Parent node index per node (`-1` for the root). */
  parent: Int32Array;
  children: number[][];
  /** Edge endpoint pairs `[parent, child, ...]` for the layout. */
  edges: Uint32Array;
}

/**
 * Index the root and its descendants. A node whose parent is missing (the server's node cap cut the graph)
 * hangs off the root, like the tree view does.
 */
export function indexGraph(root: InviteNode, descendants: readonly InviteNode[]): GraphIndex {
  const nodes = [root, ...descendants.filter((n) => n.account_id !== root.account_id)];
  const index = new Map<string, number>();
  nodes.forEach((n, i) => index.set(n.account_id, i));
  const parent = new Int32Array(nodes.length).fill(-1);
  const children: number[][] = nodes.map(() => []);
  const edges: number[] = [];
  for (let i = 1; i < nodes.length; i++) {
    const p = nodes[i]!.parent_id ? index.get(nodes[i]!.parent_id!) : undefined;
    const at = p !== undefined && p !== i ? p : 0;
    parent[i] = at;
    children[at]!.push(i);
    edges.push(at, i);
  }
  return {
    ids: nodes.map((n) => n.account_id),
    nodes,
    index,
    parent,
    children,
    edges: Uint32Array.from(edges),
  };
}

/** Nodes hidden because an ancestor is collapsed (the collapsed node itself stays visible). */
export function hiddenBy(g: GraphIndex, collapsed: ReadonlySet<string>): Uint8Array {
  const hidden = new Uint8Array(g.nodes.length);
  const stack: number[] = [];
  for (const id of collapsed) {
    const at = g.index.get(id);
    if (at !== undefined) stack.push(...g.children[at]!);
  }
  while (stack.length > 0) {
    const n = stack.pop()!;
    if (hidden[n]) continue;
    hidden[n] = 1;
    stack.push(...g.children[n]!);
  }
  return hidden;
}

/** How many nodes sit below `index` (its whole subtree, without itself). */
export function subtreeSize(g: GraphIndex, index: number): number {
  let n = 0;
  const stack = [...g.children[index]!];
  while (stack.length > 0) {
    const c = stack.pop()!;
    n += 1;
    stack.push(...g.children[c]!);
  }
  return n;
}

/** Toggle a collapsed node; a leaf cannot be collapsed. */
export function toggleCollapsed(
  g: GraphIndex,
  collapsed: ReadonlySet<string>,
  id: string,
): Set<string> {
  const next = new Set(collapsed);
  const at = g.index.get(id);
  if (at === undefined || g.children[at]!.length === 0) return next;
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

// ---- the view ---------------------------------------------------------------------------------------

/** World to screen: `screen = world * k + (x, y)`. */
export interface View {
  x: number;
  y: number;
  k: number;
}

export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 12;

export const clampZoom = (k: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, k));

export const toWorld = (v: View, sx: number, sy: number): [number, number] => [
  (sx - v.x) / v.k,
  (sy - v.y) / v.k,
];

/** Zoom by `factor` keeping the world point under the screen point (sx, sy) fixed. */
export function zoomAt(v: View, sx: number, sy: number, factor: number): View {
  const k = clampZoom(v.k * factor);
  const [wx, wy] = toWorld(v, sx, sy);
  return { k, x: sx - wx * k, y: sy - wy * k };
}

/** A view that fits the visible nodes into `width` x `height` with a margin. */
export function fitView(
  pos: Float32Array,
  hidden: Uint8Array,
  width: number,
  height: number,
  margin = 32,
): View {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (let i = 0; i < hidden.length; i++) {
    if (hidden[i]) continue;
    const x = pos[2 * i]!;
    const y = pos[2 * i + 1]!;
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  }
  if (!Number.isFinite(minX)) return { x: width / 2, y: height / 2, k: 1 };
  const w = Math.max(maxX - minX, 1);
  const h = Math.max(maxY - minY, 1);
  const k = clampZoom(Math.min((width - 2 * margin) / w, (height - 2 * margin) / h));
  return {
    k,
    x: width / 2 - ((minX + maxX) / 2) * k,
    y: height / 2 - ((minY + maxY) / 2) * k,
  };
}

/** Node radius in world units, from the number of children (a hub reads bigger). */
export const nodeRadius = (childCount: number) => 4 + Math.min(8, Math.sqrt(childCount) * 1.6);

/** The visible node under the screen point, the nearest one within its radius plus a small slop. */
export function hitTest(
  g: GraphIndex,
  pos: Float32Array,
  hidden: Uint8Array,
  v: View,
  sx: number,
  sy: number,
  slopPx = 3,
): number {
  const [wx, wy] = toWorld(v, sx, sy);
  let best = -1;
  let bestD = Infinity;
  for (let i = 0; i < g.nodes.length; i++) {
    if (hidden[i]) continue;
    const dx = pos[2 * i]! - wx;
    const dy = pos[2 * i + 1]! - wy;
    const d = Math.hypot(dx, dy);
    const r = nodeRadius(g.children[i]!.length) + slopPx / v.k;
    if (d <= r && d < bestD) {
      best = i;
      bestD = d;
    }
  }
  return best;
}

// ---- appearance -------------------------------------------------------------------------------------

export type NodeLook = {
  /** CSS custom property that holds the fill, with its fallback colour. */
  token: string;
  fallback: string;
  /** Draw a cross over the node (banned). */
  banned: boolean;
  /** Halo width in px, 0 for none (suspicion). */
  halo: number;
};

const LOOKS: Record<string, [string, string]> = {
  active: ['--ui-status-online', '#3ba55d'],
  deactivated: ['--ui-status-idle', '#e0a526'],
  platform_banned: ['--ui-status-dnd', '#d83c3e'],
  deleted: ['--ui-status-offline', '#747f8d'],
};

/** Colour by account state, a halo for a suspicion level above zero, a cross for a ban. */
export function lookOf(n: Pick<InviteNode, 'state' | 'suspicion_level'>): NodeLook {
  const [token, fallback] = LOOKS[n.state] ?? LOOKS.deleted!;
  return {
    token,
    fallback,
    banned: n.state === 'platform_banned',
    halo: n.suspicion_level > 0 ? 2 + Math.min(n.suspicion_level, 5) * 1.5 : 0,
  };
}
