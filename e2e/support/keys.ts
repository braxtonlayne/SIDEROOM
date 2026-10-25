import { expect, type Locator, type Page } from '@playwright/test';

/**
 * Helpers for the keyboard-only specs (RDY-A11Y-003). Nothing here clicks or fills: focus moves with the
 * keyboard, and every step asserts where focus is and what the live regions said.
 */

/**
 * Whether Tab reaches links in this browser. Safari and some WebKit builds skip links on Tab (a system
 * preference; Option+Tab reaches them), Chromium, Firefox and WebKitGTK on Linux do not. Specs whose flow
 * needs a link skip with a reason instead of failing on that preference.
 */
export async function tabsToLinks(page: Page): Promise<boolean> {
  const probe = await page.context().newPage();
  try {
    await probe.setContent('<a href="#x">link</a><button>button</button>');
    await probe.keyboard.press('Tab');
    return await probe.evaluate(() => document.activeElement?.tagName === 'A');
  } finally {
    await probe.close();
  }
}

export const LINKS_NOT_TABBABLE = 'This browser build does not Tab to links (the Safari default).';

export const isFocused = (loc: Locator) => loc.evaluate((el) => el === document.activeElement);

/**
 * Presses Tab (or Shift+Tab) until `target` has focus and returns how many presses it took. Fails after `max`.
 * The count is not asserted exactly (browsers differ on which controls are tab stops); only reachability is.
 */
export async function tabTo(
  page: Page,
  target: Locator,
  { max = 60, back = false }: { max?: number; back?: boolean } = {},
): Promise<number> {
  for (let i = 1; i <= max; i++) {
    await page.keyboard.press(back ? 'Shift+Tab' : 'Tab');
    if (await isFocused(target).catch(() => false)) return i;
  }
  throw new Error(`${target} was not reached with ${max} ${back ? 'Shift+Tab' : 'Tab'} presses`);
}

/** The accessible description of whatever has focus: tag, role, and name, for failure messages and asserts. */
export const focusedName = (page: Page) =>
  page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    if (!el) return '';
    return el.getAttribute('aria-label') ?? el.textContent?.trim().slice(0, 80) ?? el.tagName;
  });

/** True when the focused element is inside `container`. */
export const focusWithin = (_page: Page, container: Locator) =>
  container.evaluate((el) => el.contains(document.activeElement));

/**
 * Records every non-empty text the two app live regions (`<LiveRegions />`) take, from now on, so a test can
 * assert an announcement that was replaced by the next one. The regions clear after 8 s and re-fill on the next
 * tick, which a plain `toHaveText` can miss.
 */
export async function watchLive(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as unknown as { __live: { polite: string[]; assertive: string[] } };
    w.__live = { polite: [], assertive: [] };
    const read = () => {
      const root = document.querySelector('[data-testid="live-regions"]');
      if (!root) return;
      const map = {
        polite: root.querySelector('[role="status"]'),
        assertive: root.querySelector('[role="alert"]'),
      } as const;
      for (const k of ['polite', 'assertive'] as const) {
        const text = map[k]?.textContent?.trim();
        const list = w.__live[k];
        if (text && list[list.length - 1] !== text) list.push(text);
      }
    };
    new MutationObserver(read).observe(document, {
      subtree: true,
      childList: true,
      characterData: true,
    });
  });
}

export type Region = 'polite' | 'assertive';

export const heard = (page: Page, region: Region = 'polite') =>
  page.evaluate(
    (r) => (window as unknown as { __live?: Record<string, string[]> }).__live?.[r] ?? [],
    region,
  );

/** Waits until a live region has said something matching `text`. */
export async function expectHeard(
  page: Page,
  text: string | RegExp,
  region: Region = 'polite',
): Promise<void> {
  await expect
    .poll(
      async () =>
        (await heard(page, region)).some((h) =>
          typeof text === 'string' ? h.includes(text) : text.test(h),
        ),
      {
        message: `the ${region} live region should have said ${String(text)}; it said ${JSON.stringify(await heard(page, region))}`,
        timeout: 15_000,
      },
    )
    .toBe(true);
}

/** Records every text an element (found by selector) takes, for status lines that are replaced quickly. */
export async function watchText(page: Page, selector: string, key: string): Promise<void> {
  await page.addInitScript(
    ({ selector, key }) => {
      const w = window as unknown as Record<string, string[]>;
      w[key] = [];
      const read = () => {
        const text = document.querySelector(selector)?.textContent?.trim();
        if (text && w[key]![w[key]!.length - 1] !== text) w[key]!.push(text);
      };
      new MutationObserver(read).observe(document, {
        subtree: true,
        childList: true,
        characterData: true,
      });
    },
    { selector, key },
  );
}

export const textHistory = (page: Page, key: string) =>
  page.evaluate((k) => (window as unknown as Record<string, string[]>)[k] ?? [], key);

/** The modifier for app shortcuts: Ctrl on Windows and Linux, Cmd on macOS (the apps checks `metaKey` or `ctrlKey`). */
export const MOD = 'Control';
