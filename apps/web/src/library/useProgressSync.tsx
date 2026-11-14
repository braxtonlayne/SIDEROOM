// The reader and the account's other devices (D-690, LIB-RDR-018). One hook for the image, PDF and EPUB readers:
// it owns saving a position and what happens when the server has a newer one.
//
//  - A save names the position it started from (`baseUpdatedAtMs`): the server's `updated_at_ms` from `get` on
//    open, then from every applied save's answer. Saves go one after the other so the base is always the last answer.
//  - `applied: false` means another device saved since. If it is not further than here, nothing is lost by
//    writing over it and it is done quietly. If it is further, the reader stops saving, asks once ("Continue from
//    page N" / "Stay here"), and keeps what it had until the person chooses. A save during the question, and the
//    save on leaving the page, write nothing.
//  - While the chapter is open, `SR_LIBRARY_UPDATE` (progressEvents.ts) can say another device went further: a
//    notice offers the jump. The page never moves by itself, and a position already declined is not offered again.
import { ConfirmDialog, Icon } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { useT } from '../i18n';
import { onRemoteProgress, type RemoteProgress } from './progressEvents';
import { progressApi, type ProgressInput } from './progress';
import type { ChapterProgress } from './types';

export interface ProgressSyncOptions {
  chapterId: string;
  /** The `updated_at_ms` of the position `get` returned on open (0 or absent: none). */
  baseMs?: number | null;
  /** `epub`: positions are told as a percent and jumped to by CFI; otherwise as pages. */
  kind: 'pages' | 'epub';
  /** Pages in the chapter (pages kind). */
  pageCount: number;
  /** Where the reader is now: 0-based page and percent. */
  current: () => { pageIndex: number; percent: number };
  /** Move the reader to a position the server holds. */
  jump: (p: ChapterProgress) => void;
}

export interface ProgressSync {
  /** Save a position. Resolves false when the request failed (the caller may try again later). */
  save(input: ProgressInput): Promise<boolean>;
  /** The question and the notice; render once inside the reader. */
  ui: ReactNode;
}

const isFurther = (kind: 'pages' | 'epub', other: ChapterProgress, here: ProgressInput) =>
  kind === 'epub' ? other.percent > here.percent : other.page_index > here.pageIndex;

export function useProgressSync(opts: ProgressSyncOptions): ProgressSync {
  const t = useT();
  const o = useRef(opts);
  o.current = opts;
  const s = useRef({
    base: opts.baseMs ?? 0,
    blocked: false,
    held: null as ProgressInput | null,
    sent: null as { pageIndex: number; cfi: string | null } | null,
    declined: 0,
    chain: Promise.resolve(),
  });
  const [conflict, setConflict] = useState<ChapterProgress | null>(null);
  const [notice, setNotice] = useState<ChapterProgress | null>(null);

  const send = useCallback((input: ProgressInput, base: number | null): Promise<boolean> => {
    const st = s.current;
    st.sent = { pageIndex: input.pageIndex, cfi: input.cfi ?? null };
    let ok = true;
    const run = async () => {
      try {
        let r = await progressApi.save({ ...input, baseUpdatedAtMs: base });
        if (r && !r.applied) {
          if (!isFurther(o.current.kind, r.server, input)) {
            // The other device is not ahead: writing over it loses nothing.
            r = await progressApi.save({ ...input, baseUpdatedAtMs: null });
          } else {
            st.blocked = true;
            st.held = input;
            setNotice(null);
            setConflict(r.server);
            return;
          }
        }
        if (r?.applied && r.progress?.updated_at_ms != null) st.base = r.progress.updated_at_ms;
      } catch {
        ok = false;
      }
    };
    const next = st.chain.then(run);
    st.chain = next;
    return next.then(() => ok);
  }, []);

  const save = useCallback(
    (input: ProgressInput): Promise<boolean> => {
      const st = s.current;
      if (st.blocked) {
        // The question is open: keep the latest position for "Stay here", write nothing.
        st.held = input;
        return Promise.resolve(true);
      }
      return send(input, st.base);
    },
    [send],
  );

  const stay = () => {
    const st = s.current;
    const held = st.held;
    st.blocked = false;
    st.held = null;
    setConflict(null);
    // No base: it applies over the further position, and the answer is the new base.
    if (held) void send(held, null);
  };
  const goThere = () => {
    const st = s.current;
    const server = conflict;
    st.blocked = false;
    st.held = null;
    setConflict(null);
    if (!server) return;
    st.base = server.updated_at_ms ?? st.base;
    o.current.jump(server);
  };

  // Another device saved this chapter while it is open.
  const chapterId = opts.chapterId;
  useEffect(() => {
    if (!chapterId) return;
    return onRemoteProgress(chapterId, (ev: RemoteProgress) => {
      const st = s.current;
      const at = ev.updated_at_ms ?? 0;
      if (st.blocked || at <= st.base || at <= st.declined) return;
      // Our own save can reach us before its answer does: same position as the last one sent is our echo.
      if (st.sent && st.sent.pageIndex === ev.page_index && st.sent.cfi === (ev.cfi ?? null))
        return;
      const here = o.current.current();
      const further =
        o.current.kind === 'epub' ? ev.percent > here.percent : ev.page_index > here.pageIndex;
      if (further) setNotice(ev);
    });
  }, [chapterId]);

  const where = (p: ChapterProgress) =>
    opts.kind === 'epub'
      ? { percent: Math.round(p.percent) }
      : { page: p.page_index + 1, total: opts.pageCount };

  const ui = (
    <>
      {notice && !conflict && (
        <div className="lib-sync-notice" role="status">
          <span>
            {opts.kind === 'epub'
              ? t('library.sync.noticePercent', where(notice))
              : t('library.sync.noticePage', where(notice))}
          </span>
          <button
            type="button"
            className="lib-btn lib-btn-gold"
            onClick={() => {
              const n = notice;
              setNotice(null);
              s.current.base = n.updated_at_ms ?? s.current.base;
              o.current.jump(n);
            }}
          >
            {opts.kind === 'epub'
              ? t('library.sync.jumpPercent', where(notice))
              : t('library.sync.jumpPage', where(notice))}
          </button>
          <button
            type="button"
            className="lib-btn"
            aria-label={t('library.sync.dismiss')}
            onClick={() => {
              s.current.declined = notice.updated_at_ms ?? 0;
              setNotice(null);
            }}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      )}
      <ConfirmDialog
        open={conflict !== null}
        title={t('library.sync.title')}
        description={
          conflict
            ? opts.kind === 'epub'
              ? t('library.sync.bodyPercent', where(conflict))
              : t('library.sync.bodyPage', where(conflict))
            : undefined
        }
        confirmLabel={
          conflict
            ? opts.kind === 'epub'
              ? t('library.sync.continuePercent', where(conflict))
              : t('library.sync.continuePage', where(conflict))
            : undefined
        }
        cancelLabel={t('library.sync.stay')}
        onConfirm={goThere}
        onCancel={stay}
      />
    </>
  );
  return { save, ui };
}
