import type { Locator, Page } from '@playwright/test';

/**
 * Shared touch-target probe (wave 92, SEC-AUD-404, D-917).
 *
 * D-856 asks that, at 375 px with touch, for a control with box centre (cx, cy), `document.elementFromPoint` at
 * (cx, cy - 21) and (cx, cy + 21) each return the control, a node inside it, or its own `<label>`.
 *
 * The story behind this file is SEC-AUD-393: wave 90's probe hit-tested the selects inside Profile's closed
 * "Advanced" `<details>`. A closed `<details>` hides its content, the control cannot be scrolled to, and
 * `elementFromPoint` returns null; the plan called that a fault and wave 91 spent a builder finding there was none.
 * A null is not a fault. So: `openCollapsed` opens what is closed (by real clicks), `hitTest` scrolls the control to the
 * middle of the viewport and measures, and a control that cannot be reached throws `ProbeError` ("not reached"),
 * which is never reported as a failing control.
 *
 * Use (three lines in a spec):
 *   const opened = await openCollapsed(page, page.getByRole('main'));
 *   const rows = await hitTestAll(page, page.getByRole('main'));
 *   expect(rows.filter((r) => r.status !== 'pass' && r.status !== 'skipped')).toEqual([]);
 * An open menu (items are `role="menuitem"`, not buttons; D-941): add them with the `also` option:
 *   const rows = await hitTestAll(page, menu, { also: MENU_ITEM_SELECTOR });
 *
 * Reach (wave 103, SEC-AUD-467, D-983). The default, `reach: 'touch'`, is the 44 px rule above: points 21 px from the
 * centre. A control shorter than 44 px then "fails" at pointer widths (the Arcade's 28 px buttons at 1024) in the base
 * and after alike, so such a count cannot show a regression. `reach: 'box'` puts the points just inside the control's
 * own top and bottom edge (offset `min(21, floor(height / 2) - 1)`, never under 1): it asks "does another element take
 * a point of my own box", not "am I 44 px tall". Use the default at 375 with touch, always (that is where D-856
 * applies); use `'box'` at pointer widths (1024). A pass with `'box'` never shows a control is 44 px, and a fail with
 * a taker named is a fault at either reach. Every result reports the offset used (`offset`) and the control's height
 * (`box.height`), so a reader can tell the two reaches apart. A number is still accepted as `reach` (a fixed offset).
 *   const rows = await hitTestAll(page, root, { reach: 'box' });
 *
 * Names (wave 105, D-1012). A row's and a taker's `name` is, in order: `aria-label`; `aria-labelledby`; the text of the
 * control's own `<label>` (`labels` for a labelable element, with the control itself left out; else the closest
 * `label`); its own text; `placeholder`; `title`. Folded to single spaces, 40 characters. The name never changes a
 * status, a taker or a count.
 *
 * Plain Playwright and DOM; nothing is imported from a spec.
 */

/** A control that could not be measured (no match, no box, off screen, or a null where the centre should be). */
export class ProbeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ProbeError';
  }
}

export type PointName = 'centre' | 'above' | 'below' | 'left' | 'right';
export type Relation = 'control' | 'inside' | 'wrapper' | 'label';

/** What `elementFromPoint` gave. */
export interface Taker {
  tag: string;
  role: string | null;
  name: string;
  path: string;
}

export interface PointResult {
  at: PointName;
  x: number;
  y: number;
  /** null: the point is not on the page (a probe error, thrown before a result is returned). */
  taker: Taker | null;
  /** Which part of the control took the point; null when another node took it. */
  relation: Relation | null;
  ok: boolean;
}

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HitResult {
  box: Box;
  /** Distance of the above/below points from the centre, as used (21 for 'touch'). */
  offset: number;
  points: PointResult[];
  pass: boolean;
}

export interface HitOptions {
  /** Also test 21 px left and right of the centre. */
  sides?: boolean;
  /** A selector: a node inside the control's closest match of it counts as the control's own. */
  wrapper?: string;
  /**
   * `'touch'` (default): points 21 px from the centre, D-856, for 375 with touch. `'box'`: points just inside the
   * control's own edges, `min(21, floor(height / 2) - 1)` px from the centre, for pointer widths. A number: that offset.
   */
  reach?: 'touch' | 'box' | number;
  /** A selector: nodes under the root that match it are tested as well as the built-in controls (D-941). */
  also?: string;
}

export interface Opened {
  kind: 'details' | 'aria-expanded';
  text: string;
}

const MENU_ROLES = ['menu', 'listbox', 'combobox', 'dialog', 'tooltip'];

/**
 * Opens what is closed under `root`: every `details:not([open])` by clicking its `<summary>`, and every button with
 * `aria-expanded="false"` whose `aria-controls` names an element inside `root`, except the triggers of a menu,
 * listbox, combobox, dialog or tooltip (`aria-haspopup`, such a role): those open overlays, not cards. Three passes at
 * most (an inner card shows only after the outer one is open), then two animation frames. Returns what it opened.
 */
export async function openCollapsed(page: Page, root: Locator): Promise<Opened[]> {
  const opened: Opened[] = [];
  for (let pass = 0; pass < 3; pass++) {
    let n = 0;
    const summaries = await root.locator('details:not([open]) > summary').all();
    for (const s of summaries) {
      if (!(await s.isVisible())) continue; // inside a closed one: the next pass
      const text = ((await s.textContent()) ?? '').trim().slice(0, 40);
      await s.scrollIntoViewIfNeeded();
      await s.click({ timeout: 5000 });
      opened.push({ kind: 'details', text });
      n++;
    }
    const buttons = await root.locator('button[aria-expanded="false"][aria-controls]').all();
    for (const b of buttons) {
      if (!(await b.isVisible())) continue;
      const info = await b.evaluate((el, roles) => {
        const popup = el.getAttribute('aria-haspopup');
        const role = (el.getAttribute('role') ?? '').toLowerCase();
        return {
          overlay: (popup !== null && popup !== 'false') || roles.includes(role),
          controls: el.getAttribute('aria-controls') ?? '',
          text: (el.textContent ?? '').trim().slice(0, 40),
        };
      }, MENU_ROLES);
      const inside =
        info.controls !== '' && (await root.locator(`[id="${info.controls}"]`).count()) > 0;
      if (info.overlay || !inside) continue;
      await b.scrollIntoViewIfNeeded();
      await b.click({ timeout: 5000 });
      opened.push({ kind: 'aria-expanded', text: info.text });
      n++;
    }
    if (n === 0) break;
    await settle(page);
  }
  await settle(page);
  return opened;
}

async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))),
  );
}

interface InPageResult {
  box: Box;
  offset: number;
  points: PointResult[];
  vw: number;
  vh: number;
}

/**
 * Scrolls the control to the middle of the viewport, reads its box and returns what `elementFromPoint` gave at the
 * centre, `offset` px above and below (`opts.reach`: 21 for 'touch', inside the box for 'box') (and left and right with `opts.sides`). Throws `ProbeError` when the locator
 * matches nothing, the box is empty or outside the viewport after scrolling, or a point returns null. A point taken by
 * another node is a real result: `pass: false`, the taker named.
 */
export async function hitTest(
  _page: Page,
  locator: Locator,
  opts: HitOptions = {},
): Promise<HitResult> {
  const count = await locator.count();
  if (count === 0) throw new ProbeError(`no element matches ${locator.toString()}`);
  const r = await locator.first().evaluate(inPage, {
    sides: !!opts.sides,
    wrapper: opts.wrapper ?? null,
    reach: opts.reach ?? 'touch',
  });
  if (r.box.width <= 0 || r.box.height <= 0)
    throw new ProbeError(`empty box ${r.box.width}x${r.box.height}: ${locator.toString()}`);
  const cx = r.box.x + r.box.width / 2;
  const cy = r.box.y + r.box.height / 2;
  if (cx < 0 || cy < 0 || cx > r.vw || cy > r.vh)
    throw new ProbeError(
      `centre (${Math.round(cx)}, ${Math.round(cy)}) is outside the ${r.vw}x${r.vh} viewport after scrolling`,
    );
  const centre = r.points.find((p) => p.at === 'centre');
  if (!centre || !centre.taker)
    throw new ProbeError(
      `the centre returns no element (a closed container, or not on the page): ${locator.toString()}`,
    );
  const gone = r.points.find((p) => !p.taker);
  if (gone)
    throw new ProbeError(
      `the point ${gone.at} (${Math.round(gone.x)}, ${Math.round(gone.y)}) is off the page`,
    );
  return { box: r.box, offset: r.offset, points: r.points, pass: r.points.every((p) => p.ok) };
}

/** Runs in the page. Kept free of closures: Playwright serialises it. */
function inPage(
  el: Element,
  a: { sides: boolean; wrapper: string | null; reach: 'touch' | 'box' | number },
): InPageResult {
  (el as HTMLElement).scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
  const b = el.getBoundingClientRect();
  const box = { x: b.left, y: b.top, width: b.width, height: b.height };
  const cx = b.left + b.width / 2;
  const cy = b.top + b.height / 2;
  const offset =
    typeof a.reach === 'number'
      ? a.reach
      : a.reach === 'box'
        ? Math.max(1, Math.min(21, Math.floor(b.height / 2) - 1))
        : 21;
  const own = (t: Element | null): Relation | null => {
    if (!t) return null;
    if (t === el) return 'control';
    if (el.contains(t)) return 'inside';
    let wrap: Element | null = null;
    if (a.wrapper) wrap = el.closest(a.wrapper);
    else if (
      el instanceof HTMLInputElement &&
      (el.type === 'checkbox' || el.type === 'radio' || el.getAttribute('role') === 'switch')
    )
      wrap = el.parentElement;
    if (wrap && wrap.contains(t)) return 'wrapper';
    const lab =
      el.closest('label') ??
      ((el as HTMLElement).id
        ? document.querySelector(`label[for="${CSS.escape((el as HTMLElement).id)}"]`)
        : null);
    if (lab && (t === lab || lab.contains(t))) return 'label';
    return null;
  };
  const name = (e: Element): string => {
    const fold = (x: string) => x.trim().replace(/\s+/g, ' ').slice(0, 40);
    const aria = e.getAttribute('aria-label');
    if (aria) return aria.trim().slice(0, 40);
    const by = e.getAttribute('aria-labelledby');
    if (by) {
      const txt = fold(
        by
          .split(/\s+/)
          .map((id) => document.getElementById(id)?.textContent ?? '')
          .join(' '),
      );
      if (txt) return txt;
    }
    // D-1012: the text of the control's own <label> (`labels` for a labelable element, with the control itself left out), else the closest label.
    const labels = (e as HTMLInputElement).labels;
    const lab = labels ? labels[0] : e instanceof HTMLLabelElement ? null : e.closest('label');
    if (lab) {
      const text = (n: Node): string =>
        n === e
          ? ''
          : n.nodeType === 3
            ? (n.textContent ?? '')
            : Array.from(n.childNodes).map(text).join(' ');
      const txt = fold(labels ? text(lab) : (lab.textContent ?? ''));
      if (txt) return txt;
    }
    const own = fold(e.textContent ?? '');
    if (own) return own;
    return (e.getAttribute('placeholder') ?? e.getAttribute('title') ?? '').slice(0, 40);
  };
  const path = (e: Element): string => {
    const parts: string[] = [];
    for (let n: Element | null = e; n && parts.length < 4; n = n.parentElement) {
      const cls = typeof n.className === 'string' ? (n.className.trim().split(/\s+/)[0] ?? '') : '';
      parts.push(
        n.tagName.toLowerCase() +
          (cls ? `.${cls.replace(/_[a-z0-9]{4,}$/i, '').slice(0, 24)}` : ''),
      );
    }
    return parts.join(' < ');
  };
  const spec: [PointName, number, number][] = [
    ['centre', cx, cy],
    ['above', cx, cy - offset],
    ['below', cx, cy + offset],
  ];
  if (a.sides) {
    spec.push(['left', cx - offset, cy], ['right', cx + offset, cy]);
  }
  const points = spec.map(([at, x, y]) => {
    const t = document.elementFromPoint(x, y);
    const rel = own(t);
    return {
      at,
      x,
      y,
      taker: t
        ? {
            tag: t.tagName.toLowerCase(),
            role: t.getAttribute('role'),
            name: name(t),
            path: path(t),
          }
        : null,
      relation: rel,
      ok: rel !== null,
    };
  });
  return { box, offset, points, vw: window.innerWidth, vh: window.innerHeight };
}

export interface ControlRow {
  index: number;
  tag: string;
  role: string | null;
  type: string | null;
  name: string;
  path: string;
  /** pass, fail (another node took a point), not-reached (a ProbeError), skipped (not displayed, or disabled). */
  status: 'pass' | 'fail' | 'not-reached' | 'skipped';
  reason?: string;
  result?: HitResult;
}

export const CONTROL_SELECTOR = [
  'button',
  'a[href]',
  'input:not([type=hidden]):not([type=file])',
  'textarea',
  'select',
  '[role=button]',
  '[role=link]',
  '[role=combobox]',
  '[role=checkbox]',
  '[role=switch]',
  '[role=radio]',
].join(',');

/** The items of a kit menu (D-941): pass as `also` to `hitTestAll`. */
export const MENU_ITEM_SELECTOR = '[role=menuitem],[role=menuitemcheckbox],[role=menuitemradio]';

/**
 * Every enabled button, link, kit select trigger, checkbox, switch, radio and text field under `root`, one row each.
 * A control that is not displayed (`display: none` up the tree) or is disabled is `skipped`, except when the thing
 * hiding it is the panel of a collapsed button (`aria-expanded="false"`): that is `not-reached`, because the caller
 * forgot `openCollapsed`. A `ProbeError` on one control is collected, never thrown; the caller decides (a spec fails on
 * any `not-reached`).
 */
export async function hitTestAll(
  page: Page,
  root: Locator,
  opts: HitOptions = {},
): Promise<ControlRow[]> {
  const all = root.locator(opts.also ? `${CONTROL_SELECTOR},${opts.also}` : CONTROL_SELECTOR);
  const n = await all.count();
  const rows: ControlRow[] = [];
  for (let i = 0; i < n; i++) {
    const loc = all.nth(i);
    const info = await loc.evaluate((el) => {
      const cls = (e: Element) =>
        typeof e.className === 'string' ? (e.className.trim().split(/\s+/)[0] ?? '') : '';
      let path = '';
      for (let x: Element | null = el, k = 0; x && k < 3; x = x.parentElement, k++)
        path += `${k ? ' < ' : ''}${x.tagName.toLowerCase()}${
          cls(x)
            ? `.${cls(x)
                .replace(/_[a-z0-9]{4,}$/i, '')
                .slice(0, 24)}`
            : ''
        }`;
      const nameOf = (e: Element): string => {
        const fold = (x: string) => x.trim().replace(/\s+/g, ' ').slice(0, 40);
        const aria = e.getAttribute('aria-label');
        if (aria) return aria.trim().slice(0, 40);
        const by = e.getAttribute('aria-labelledby');
        if (by) {
          const txt = fold(
            by
              .split(/\s+/)
              .map((id) => document.getElementById(id)?.textContent ?? '')
              .join(' '),
          );
          if (txt) return txt;
        }
        // D-1012: the text of the control's own <label> (`labels` for a labelable element, with the control itself left out), else the closest label.
        const labels = (e as HTMLInputElement).labels;
        const lab = labels ? labels[0] : e instanceof HTMLLabelElement ? null : e.closest('label');
        if (lab) {
          const text = (n: Node): string =>
            n === e
              ? ''
              : n.nodeType === 3
                ? (n.textContent ?? '')
                : Array.from(n.childNodes).map(text).join(' ');
          const txt = fold(labels ? text(lab) : (lab.textContent ?? ''));
          if (txt) return txt;
        }
        const own = fold(e.textContent ?? '');
        if (own) return own;
        return (e.getAttribute('placeholder') ?? e.getAttribute('title') ?? '').slice(0, 40);
      };
      const name = nameOf(el);
      const disabled =
        (el as HTMLButtonElement).disabled === true || el.getAttribute('aria-disabled') === 'true';
      // Is something up the tree display: none, and is that thing a collapsed button's panel?
      let hiddenBy: 'none' | 'display' | 'collapsed' = 'none';
      for (let x: Element | null = el; x; x = x.parentElement) {
        if (getComputedStyle(x).display === 'none') {
          const closedTrigger =
            x.id !== '' &&
            document.querySelector(`[aria-controls="${CSS.escape(x.id)}"][aria-expanded="false"]`);
          hiddenBy = closedTrigger ? 'collapsed' : 'display';
          break;
        }
      }
      return {
        tag: el.tagName.toLowerCase(),
        role: el.getAttribute('role'),
        type: el instanceof HTMLInputElement ? el.type : null,
        name,
        path,
        disabled,
        hiddenBy,
      };
    });
    const base = {
      index: i,
      tag: info.tag,
      role: info.role,
      type: info.type,
      name: info.name,
      path: info.path,
    };
    if (info.disabled) {
      rows.push({ ...base, status: 'skipped', reason: 'disabled' });
      continue;
    }
    if (info.hiddenBy === 'display') {
      rows.push({ ...base, status: 'skipped', reason: 'not displayed' });
      continue;
    }
    if (info.hiddenBy === 'collapsed') {
      rows.push({
        ...base,
        status: 'not-reached',
        reason: 'inside a collapsed panel (openCollapsed was not run)',
      });
      continue;
    }
    try {
      const result = await hitTest(page, loc, opts);
      rows.push({ ...base, status: result.pass ? 'pass' : 'fail', result });
    } catch (e) {
      if (!(e instanceof ProbeError)) throw e;
      rows.push({ ...base, status: 'not-reached', reason: e.message.slice(0, 300) });
    }
  }
  return rows;
}
