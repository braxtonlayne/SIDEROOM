import { lazy, Suspense, useEffect } from 'react';
import { matchCustomShortcut } from '../lounge/shortcuts';
import { useClientFlag } from '../moderation/reports';
import { usePrefsStore } from '../settings/user/prefs';
import { canCapture, captureTab } from './capture';
import { collectPrivateRects } from './covers';
import { BETA_REPORTS_FLAG } from './flag';
import { grabFrame } from './grab';
import { bugReportRequest, closeBugReport, openBugReport, useBugReportRequest } from './openState';

const BugReportDialog = lazy(() =>
  import('./BugReportDialog').then((m) => ({ default: m.BugReportDialog })),
);

let capturing = false;

/**
 * The key's whole job. `captureTab` calls `getDisplayMedia` before its first await, so this stays inside the key
 * press's user gesture; the private rectangles are read first, while the page is as the member sees it.
 */
function startFromKey(): void {
  if (capturing || bugReportRequest().open) return;
  const rects = collectPrivateRects(document);
  const viewport = { w: window.innerWidth, h: window.innerHeight };
  if (!canCapture(navigator.mediaDevices)) {
    openBugReport({ rects, viewport, note: 'unsupported' });
    return;
  }
  capturing = true;
  void captureTab(navigator.mediaDevices, grabFrame)
    .then((res) =>
      openBugReport(
        res.ok ? { frame: res.frame, rects, viewport } : { rects, viewport, note: res.reason },
      ),
    )
    .finally(() => {
      capturing = false;
    });
}

/**
 * SEC-AUD-168 (D-647): the beta bug reporter's gate. Its own small chunk, rendered by the router on app routes only,
 * like `DrawerGate` (D-385: nothing of it loads before sign-in). With the platform switch off it renders nothing and
 * listens to nothing. With it on it listens for the key and for an open request from a menu entry, and loads the
 * dialog chunk the first time one comes.
 */
export function BugReportGate() {
  const on = useClientFlag(BETA_REPORTS_FLAG);
  const req = useBugReportRequest();

  useEffect(() => {
    if (!on) return;
    const onKey = (e: KeyboardEvent) => {
      const id = matchCustomShortcut(e, usePrefsStore.getState().prefs.keybinds);
      if (id !== 'bugReport') return;
      e.preventDefault();
      if (e.repeat) return;
      startFromKey();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [on]);

  // The operator turned the switch off while the dialog was open: it goes away.
  useEffect(() => {
    if (!on) closeBugReport();
  }, [on]);

  if (!on || !req.open) return null;
  return (
    <Suspense fallback={null}>
      <BugReportDialog key={req.nonce} start={req.start} onClose={closeBugReport} />
    </Suspense>
  );
}
