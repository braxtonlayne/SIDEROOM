import { Button, Modal, Notice, TextArea, ThemeProvider } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from 'react';
import { ApiError } from '../api/client';
import { useT, type MessageKey } from '../i18n';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { sendBugReport } from './api';
import { buildBundle } from './bundle';
import { canCapture, captureTab } from './capture';
import { MAX_DESCRIPTION, type BugBundle, type BugReportBody } from './contract';
import { collectPrivateRects, coversFromRects, frameMatchesViewport, type Rect } from './covers';
import { domCanvas, PictureTooLargeError, renderJpeg } from './encode';
import { grabFrame } from './grab';
import type { BugReportStart } from './openState';
import { PictureEditor, type PictureSource } from './PictureEditor';
import { liveSources, readServiceWorkerState } from './sources';
import './bugreport.css';

/** Largest attached file read into the preview (it is re-encoded to at most 2 MiB before it leaves). */
const MAX_FILE_BYTES = 30 * 1024 * 1024;

interface Picture extends PictureSource {
  /** True when the picture is a tab capture (the only kind the automatic covers can apply to). */
  fromCapture: boolean;
}

const nextFrames = (n: number, extraMs = 0) =>
  new Promise<void>((res) => {
    const step = (left: number) =>
      left <= 0 ? void setTimeout(res, extraMs) : void requestAnimationFrame(() => step(left - 1));
    step(n);
  });

async function loadImageFile(file: File): Promise<PictureSource> {
  if (typeof createImageBitmap === 'function') {
    const bmp = await createImageBitmap(file);
    return { source: bmp, w: bmp.width, h: bmp.height };
  }
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    return { source: img, w: img.naturalWidth, h: img.naturalHeight };
  } finally {
    URL.revokeObjectURL(url);
  }
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(r.error ?? new Error('Could not read the picture'));
    r.onload = () => {
      const url = String(r.result);
      resolve(url.slice(url.indexOf(',') + 1));
    };
    r.readAsDataURL(blob);
  });
}

/** The report's number as the member sees it: a numeric id as is, an opaque one shortened. */
const reportNumber = (id: string) =>
  /^\d{1,12}$/.test(id) ? id : id.replace(/-/g, '').slice(0, 8);

function failureKey(e: unknown): MessageKey {
  if (e instanceof PictureTooLargeError) return 'bugreport.errorPicture';
  if (e instanceof ApiError) {
    if (e.status === 429) return 'bugreport.errorLimit';
    if (e.status === 413) return 'bugreport.errorTooLarge';
    if (e.status === 403 || e.status === 404) return 'bugreport.errorOff';
    return 'bugreport.errorGeneric';
  }
  if (typeof navigator !== 'undefined' && navigator.onLine === false)
    return 'bugreport.errorOffline';
  if (e instanceof TypeError) return 'bugreport.errorOffline';
  return 'bugreport.errorGeneric';
}

function BugReportForm({ start, onClose }: { start: BugReportStart | null; onClose: () => void }) {
  const t = useT();
  const router = useRouter();
  const [description, setDescription] = useState('');
  const [picture, setPicture] = useState<Picture | null>(() =>
    start?.frame
      ? { source: start.frame.source, w: start.frame.w, h: start.frame.h, fromCapture: true }
      : null,
  );
  const [covers, setCovers] = useState<Rect[]>(() => {
    const f = start?.frame;
    return f && start && frameMatchesViewport(start.viewport, f)
      ? coversFromRects(start.rects, start.viewport, f)
      : [];
  });
  // Covers that came with the page (`data-sr-private`), as opposed to the member's own boxes.
  const [autoCovered, setAutoCovered] = useState(covers.length > 0);
  const [note, setNote] = useState<BugReportStart['note'] | 'file' | null>(start?.note ?? null);
  const [bundle, setBundle] = useState<BugBundle | null>(null);
  const [busy, setBusy] = useState(false);
  const [hiding, setHiding] = useState(false);
  const [error, setError] = useState<MessageKey | null>(null);
  const [sentId, setSentId] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  // Built once, when the dialog opens: what the member reads under "What is sent" is what is sent.
  const routerRef = useRef(router);
  useEffect(() => {
    let live = true;
    void readServiceWorkerState().then((sw) => {
      if (live) setBundle(buildBundle(liveSources(routerRef.current, sw)));
    });
    return () => {
      live = false;
    };
  }, []);

  const bundleJson = useMemo(() => (bundle ? JSON.stringify(bundle, null, 2) : ''), [bundle]);
  const trimmed = description.trim();
  const canSend = trimmed.length > 0 && trimmed.length <= MAX_DESCRIPTION && !!bundle && !busy;

  const takeFrame = (frame: { source: CanvasImageSource; w: number; h: number }, rects: Rect[]) => {
    const vp = { w: window.innerWidth, h: window.innerHeight };
    const auto = frameMatchesViewport(vp, frame) ? coversFromRects(rects, vp, frame) : [];
    setPicture({ ...frame, fromCapture: true });
    setCovers(auto);
    setAutoCovered(auto.length > 0);
  };

  // Called straight from the click, so the click is the user gesture `getDisplayMedia` needs.
  const captureHere = () => {
    const rects = collectPrivateRects(document);
    setNote(null);
    void captureTab(navigator.mediaDevices, async (stream) => {
      setHiding(true); // the dialog must not be in its own picture
      await nextFrames(2, 150);
      return grabFrame(stream);
    }).then((res) => {
      setHiding(false);
      if (res.ok) takeFrame(res.frame, rects);
      else setNote(res.reason);
    });
  };

  const attach = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/') || file.size > MAX_FILE_BYTES) {
      setError('bugreport.errorFile');
      return;
    }
    try {
      const img = await loadImageFile(file);
      setError(null);
      setPicture({ ...img, fromCapture: false });
      setCovers([]);
      setAutoCovered(false);
      setNote('file');
    } catch {
      setError('bugreport.errorFile');
    }
  };

  const removePicture = () => {
    setPicture(null);
    setCovers([]);
    setAutoCovered(false);
  };

  const send = async () => {
    if (!canSend || !bundle) return;
    setBusy(true);
    setError(null);
    try {
      const body: BugReportBody = { description: trimmed, bundle };
      if (picture) {
        const jpeg = await renderJpeg(picture.source, picture.w, picture.h, covers, domCanvas);
        body.screenshot = {
          type: 'image/jpeg',
          data: await blobToBase64(jpeg.blob),
          masked: covers.length,
        };
      }
      const res = await sendBugReport(body);
      setSentId(res.id);
    } catch (e) {
      setError(failureKey(e));
    } finally {
      setBusy(false);
    }
  };

  if (hiding) return null;

  if (sentId !== null) {
    return (
      <Modal
        open
        onClose={onClose}
        title={t('bugreport.sentTitle')}
        size="sm"
        footer={<Button onClick={onClose}>{t('bugreport.close')}</Button>}
      >
        <p role="status">{t('bugreport.sentBody', { n: reportNumber(sentId) })}</p>
      </Modal>
    );
  }

  const noteKey: MessageKey | null =
    note === 'declined'
      ? 'bugreport.noteDeclined'
      : note === 'failed'
        ? 'bugreport.noteFailed'
        : note === 'unsupported'
          ? 'bugreport.noteUnsupported'
          : null;

  return (
    <Modal
      open
      onClose={busy ? () => undefined : onClose}
      title={t('bugreport.title')}
      description={t('bugreport.intro')}
      size="lg"
      className="br-modal"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t('bugreport.cancel')}
          </Button>
          <Button disabled={!canSend} loading={busy} onClick={() => void send()}>
            {t('bugreport.send')}
          </Button>
        </>
      }
    >
      <form
        className="br-form"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <TextArea
          label={t('bugreport.descriptionLabel')}
          hint={t('bugreport.counter', { n: description.length, max: MAX_DESCRIPTION })}
          value={description}
          rows={4}
          maxLength={MAX_DESCRIPTION}
          required
          onChange={(e) => setDescription(e.target.value)}
        />

        <section className="br-picture" aria-label={t('bugreport.pictureTitle')}>
          <h3 className="br-h">{t('bugreport.pictureTitle')}</h3>
          {noteKey && (
            <p className="br-note" role="status">
              {t(noteKey)}
            </p>
          )}
          {picture ? (
            <>
              <PictureEditor picture={picture} covers={covers} onChange={setCovers} />
              <p className="br-hint">{t('bugreport.coverHint')}</p>
              {picture.fromCapture && autoCovered ? (
                <p className="br-hint" role="status">
                  {t('bugreport.autoCovered')}
                </p>
              ) : (
                <Notice variant="warning" className="br-warn">
                  {t('bugreport.checkPicture')}
                </Notice>
              )}
              <div className="br-row">
                <Button variant="secondary" size="sm" onClick={removePicture}>
                  {t('bugreport.removePicture')}
                </Button>
              </div>
            </>
          ) : (
            <p className="br-hint">{t('bugreport.noPicture')}</p>
          )}
          <div className="br-row">
            {canCapture(typeof navigator !== 'undefined' ? navigator.mediaDevices : null) && (
              <Button variant="secondary" size="sm" onClick={captureHere}>
                {picture ? t('bugreport.captureAgain') : t('bugreport.capture')}
              </Button>
            )}
            <Button variant="secondary" size="sm" onClick={() => fileInput.current?.click()}>
              {t('bugreport.attach')}
            </Button>
            <input
              ref={fileInput}
              type="file"
              accept="image/*"
              className="br-file"
              tabIndex={-1}
              aria-hidden="true"
              data-testid="br-file"
              onChange={(e) => void attach(e)}
            />
          </div>
        </section>

        <details className="br-sent">
          <summary>{t('bugreport.whatIsSent')}</summary>
          <p className="br-hint">{t('bugreport.whatIsSentIntro')}</p>
          <pre className="br-json" tabIndex={0} aria-label={t('bugreport.jsonLabel')}>
            {bundleJson}
          </pre>
        </details>

        <p className="br-hint br-privacy">{t('bugreport.privacy')}</p>
        {error && (
          <Notice variant="danger" role="alert">
            {t(error)}
          </Notice>
        )}
      </form>
    </Modal>
  );
}

/** The dialog inside the viewer's theme: it is mounted at the app root, outside every ThemeProvider. */
export function BugReportDialog({
  start,
  onClose,
}: {
  start: BugReportStart | null;
  onClose: () => void;
}) {
  const theme = useAppliedTheme();
  return (
    <ThemeProvider
      section="lounge"
      userGlobal={theme.userGlobal}
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={{ ...theme.style, display: 'contents' }}
      {...theme.data}
    >
      <BugReportForm start={start} onClose={onClose} />
    </ThemeProvider>
  );
}
