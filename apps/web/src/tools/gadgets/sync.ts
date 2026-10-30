/**
 * UTL-SHL-001: keeps the layout in step with the account. The account copy is the `tool_layout` object in the
 * synced settings blob (`GET/PATCH /users/@me/settings`, the same blob the preferences use; the server keeps
 * unknown keys). The browser copy (per account) loads first so the first paint never waits; when the blob
 * arrives the account copy wins; a first sign-in on a device with a layout and an account with none pushes it
 * up once. Offline or rejected writes stay in localStorage and go out with the next change.
 */
import { rest, useStore } from '../../store';
import { EMPTY_LAYOUT, isEmptyLayout, sanitizeLayout, type Layout } from './layout';
import { useToolsStore } from './store';

export const LAYOUT_KEY = 'tool_layout';
/** Account-agnostic hint the gate reads before anything loads: some account here has gadgets or pins. */
export const HINT_KEY = 'sr.tools.any';
const localKey = (account: string) => `sr.tools.layout.v1:${account}`;

const same = (a: Layout, b: Layout) => JSON.stringify(a) === JSON.stringify(b);

function readLocal(account: string): Layout {
  try {
    const raw = localStorage.getItem(localKey(account));
    return raw ? sanitizeLayout(JSON.parse(raw)) : EMPTY_LAYOUT;
  } catch {
    return EMPTY_LAYOUT;
  }
}
function writeLocal(account: string, layout: Layout): void {
  try {
    localStorage.setItem(localKey(account), JSON.stringify(layout));
    if (isEmptyLayout(layout)) localStorage.removeItem(HINT_KEY);
    else localStorage.setItem(HINT_KEY, '1');
  } catch {
    /* private mode or full: the account copy still works */
  }
}

/** Whether the gate should mount the host without being asked (a remembered gadget or pin). */
export function layoutHint(): boolean {
  try {
    return localStorage.getItem(HINT_KEY) === '1';
  } catch {
    return false;
  }
}
/** The same question for the account blob. */
export function blobHasLayout(settings: Record<string, unknown> | undefined): boolean {
  return !isEmptyLayout(sanitizeLayout(settings?.[LAYOUT_KEY]));
}

/** Starts the sync for one account; returns the stop function. */
export function startLayoutSync(account: string, debounceMs = 400): () => void {
  let stopped = false;
  let adopting = false;
  let loaded = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let lastSent: Layout | null = null;

  const local = readLocal(account);
  let pendingLocal = !isEmptyLayout(local);
  adopting = true;
  useToolsStore.getState().hydrate(local);
  adopting = false;

  const push = async () => {
    if (stopped || !loaded) return;
    const layout = useToolsStore.getState().layout;
    if (lastSent && same(lastSent, layout)) return;
    try {
      await rest.patchUserSettings({ [LAYOUT_KEY]: layout });
      lastSent = layout;
    } catch {
      /* offline or rejected: the browser copy keeps it */
    }
  };

  const adopt = (settings: Record<string, unknown> | undefined) => {
    if (!settings) return;
    const raw = settings[LAYOUT_KEY];
    loaded = true;
    if (raw !== undefined && raw !== null) {
      const next = sanitizeLayout(raw);
      lastSent = next;
      if (!same(next, useToolsStore.getState().layout)) {
        adopting = true;
        try {
          useToolsStore.getState().hydrate(next);
        } finally {
          adopting = false;
        }
        writeLocal(account, next);
      }
    } else if (pendingLocal) {
      void push();
    }
    pendingLocal = false;
  };

  const unStore = useToolsStore.subscribe((s, prev) => {
    if (adopting || s.layout === prev.layout) return;
    writeLocal(account, s.layout);
    if (!loaded) {
      pendingLocal = true;
      return;
    }
    clearTimeout(timer);
    timer = setTimeout(() => void push(), debounceMs);
  });
  let lastBlob = useStore.getState().userSettings;
  const unBlob = useStore.subscribe((s) => {
    if (s.userSettings === lastBlob) return;
    lastBlob = s.userSettings;
    adopt(s.userSettings?.settings);
  });
  if (lastBlob) adopt(lastBlob.settings);
  else
    rest
      .fetchUserSettings()
      .then((blob) => {
        if (stopped) return;
        useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', blob);
        if (!loaded) adopt(useStore.getState().userSettings?.settings ?? blob.settings);
      })
      .catch(() => {
        /* signed out or offline: the browser copy is used */
      });

  return () => {
    stopped = true;
    clearTimeout(timer);
    unStore();
    unBlob();
  };
}
