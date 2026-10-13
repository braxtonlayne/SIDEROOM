// The Reader (LIB-RDR-001..009, 013): full-screen, auto-hiding chrome, paged (single and double) and long-strip
// modes, keyboard and tap-zone navigation, zoom and fit, progress through the data layer, next/previous chapter.
import { EmptyState, Icon, Slider, Spinner } from '@sideroom/ui';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useRouter } from '@tanstack/react-router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { isAgeGateError, useAgeGate, withAgeGate } from '../../lib/ageGate';
import { getChapters, getPages, getSeries } from '../api';
import {
  resolveReaderPrefs,
  useReaderPrefsStore,
  type ReaderMode,
  type ReaderPrefs,
} from '../prefs';
import { progressApi } from '../progress';
import type { Chapter, PageManifest, Series } from '../types';
import { preloadImage } from './imageLoader';
import {
  arrowStep,
  buildSpreads,
  clamp,
  parsePageParam,
  percentAt,
  pickVariant,
  preloadWindow,
  spreadIndexOf,
  tapStep,
} from './model';
import { PageImage } from './PageImage';
import { BookmarksPanel, SettingsPanel } from './Panels';
import { useZoomPan } from './useZoomPan';

const SAVE_DEBOUNCE_MS = 2000;
const CHROME_HIDE_MS = 3000;

const isEditable = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));

export interface ReaderProps {
  chapterId: string;
  /** 1-based page from the URL. */
  page?: string;
}

export function Reader(props: ReaderProps) {
  // A new chapter is a new reader: all page state starts over.
  return <ReaderLoader key={props.chapterId} {...props} />;
}

function ReaderLoader({ chapterId, page }: ReaderProps) {
  const navigate = useNavigate();
  const router = useRouter();
  const { ensureAdult } = useAgeGate();
  const manifest = useQuery({
    queryKey: ['library', 'pages', chapterId],
    // 403 900011 -> ask "Are you 18 or older?" -> on yes the manifest is fetched once more (D-184).
    queryFn: () => withAgeGate(() => getPages(chapterId), ensureAdult),
    retry: false,
    retryOnMount: false,
    // Signed URLs expire: refetch a few minutes before they stop working.
    staleTime: 5 * 60_000,
    refetchOnWindowFocus: (q) => q.state.data !== undefined,
  });
  const seriesId = manifest.data?.chapter.series_id;
  const series = useQuery({
    queryKey: ['library', 'series', seriesId],
    queryFn: () => getSeries(seriesId!),
    enabled: !!seriesId,
  });
  const saved = useQuery({
    queryKey: ['library', 'progress', chapterId],
    queryFn: () => progressApi.get(chapterId),
    staleTime: Infinity,
  });

  const declined = isAgeGateError(manifest.error);
  useEffect(() => {
    if (!declined) return;
    // The reader stays closed (D-184): back to where the member came from (the series page), else the home.
    if (router.history.canGoBack()) router.history.back();
    else void navigate({ to: '/library', replace: true });
  }, [declined, router, navigate]);

  if (manifest.isError && !declined) {
    const gone = manifest.error instanceof ApiError && manifest.error.status === 404;
    return (
      <div className="lib-reader lib-reader-msg" data-bg="black">
        <EmptyState
          title={gone ? 'This chapter is not available' : 'We could not open this chapter'}
          description={
            gone
              ? 'It may have been unpublished or removed.'
              : 'Check your connection and try again.'
          }
          action={
            <Link to="/library" className="lib-btn">
              Back to the library
            </Link>
          }
        />
      </div>
    );
  }
  if (!manifest.data || !series.data || saved.isPending || declined) {
    return (
      <div className="lib-reader lib-reader-msg" data-bg="black">
        <Spinner label="Opening chapter" />
      </div>
    );
  }
  const count = manifest.data.pages.length;
  const urlIndex = parsePageParam(page, count);
  const fromProgress =
    saved.data && !saved.data.completed ? clamp(saved.data.page_index, 0, count - 1) : 0;
  return (
    <ReaderView
      manifest={manifest.data}
      series={series.data.series}
      initialIndex={urlIndex ?? Math.max(0, fromProgress)}
    />
  );
}

interface ViewProps {
  manifest: PageManifest;
  series: Series;
  initialIndex: number;
}

function ReaderView({ manifest, series, initialIndex }: ViewProps) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const chapter = manifest.chapter;
  const pages = manifest.pages;
  const count = pages.length;
  const prefsState = useReaderPrefsStore();
  const prefs = resolveReaderPrefs(prefsState, series.id, manifest.reading_direction);

  const [index, setIndex] = useState(initialIndex); // 0..count; `count` is the end-of-chapter card
  const [chromeOn, setChromeOn] = useState(true);
  const [panel, setPanel] = useState<null | 'settings' | 'bookmarks'>(null);
  const [scrub, setScrub] = useState<number | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [stageWidth, setStageWidth] = useState(800);

  const strip = prefs.mode === 'strip';
  const spreads = useMemo(
    () =>
      prefs.mode === 'double' ? buildSpreads(pages, prefs.coverOffset) : pages.map((_, i) => [i]),
    [pages, prefs.mode, prefs.coverOffset],
  );
  const zoom = useZoomPan(stageRef, !strip, index);

  /* ------------------------------------------------------------ chrome */
  const poke = useCallback(() => {
    setChromeOn(true);
    clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setChromeOn(false), CHROME_HIDE_MS);
  }, []);
  useEffect(() => {
    poke();
    return () => clearTimeout(hideTimer.current);
  }, [poke]);
  const chromeVisible = chromeOn || panel !== null;

  useEffect(() => {
    const el = stageRef.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => setStageWidth(el.clientWidth || 800));
    ro.observe(el);
    setStageWidth(el.clientWidth || 800);
    return () => ro.disconnect();
  }, [strip]);

  /* -------------------------------------------------------- navigation */
  const leaveTo = useCallback(
    (chapterId: string, last: boolean) =>
      void navigate({
        to: '/library/r/$chapterId/{-$page}',
        params: { chapterId, page: last ? '9999' : undefined },
      }),
    [navigate],
  );

  const goIndex = useCallback((i: number) => setIndex(clamp(i, 0, count)), [count]);

  const step = useCallback(
    (dir: 1 | -1) => {
      if (strip) {
        const next = clamp(index + dir, 0, count);
        if (dir === 1 && index >= count - 1 && index !== count) return setIndex(count);
        if (dir === 1 && index === count) {
          if (manifest.next_chapter_id) leaveTo(manifest.next_chapter_id, false);
          return;
        }
        if (dir === -1 && index === 0) {
          if (manifest.prev_chapter_id) leaveTo(manifest.prev_chapter_id, true);
          return;
        }
        return setIndex(next);
      }
      const at = index >= count ? spreads.length : spreadIndexOf(spreads, index);
      const to = at + dir;
      if (to < 0) {
        if (manifest.prev_chapter_id) leaveTo(manifest.prev_chapter_id, true);
        return;
      }
      if (to > spreads.length) {
        if (manifest.next_chapter_id) leaveTo(manifest.next_chapter_id, false);
        return;
      }
      setIndex(to === spreads.length ? count : spreads[to]![0]!);
    },
    [strip, index, count, spreads, manifest.next_chapter_id, manifest.prev_chapter_id, leaveTo],
  );

  /* --------------------------------------------------- URL + progress */
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const firstRender = useRef(true);
  useEffect(() => {
    if (firstRender.current) return;
    // Keep the address in step with the page (replace: back leaves the reader, not the last page).
    void navigateRef.current({
      to: '/library/r/$chapterId/{-$page}',
      params: { chapterId: chapter.id, page: String(Math.min(index, count - 1) + 1) },
      replace: true,
    });
  }, [index, chapter.id, count, series.id]);

  const latest = useRef({ index, dirty: false });
  latest.current.index = index;
  const flush = useCallback(() => {
    if (!latest.current.dirty || count === 0) return;
    latest.current.dirty = false;
    const i = Math.min(latest.current.index, count - 1);
    const pct = percentAt(i, count);
    void progressApi
      .save({
        chapterId: chapter.id,
        seriesId: series.id,
        seriesTitle: series.title,
        coverUrl: series.cover_url,
        chapterNumber: chapter.number,
        pageCount: count,
        pageIndex: i,
        percent: pct,
        completed: i >= count - 1,
      })
      .then(() => {
        void qc.invalidateQueries({ queryKey: ['library', 'recent'] });
        void qc.invalidateQueries({ queryKey: ['library', 'progress', chapter.id] });
        void qc.invalidateQueries({ queryKey: ['library', 'series-progress', series.id] });
      });
  }, [chapter.id, chapter.number, count, qc, series.cover_url, series.id, series.title]);

  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false;
      return;
    }
    latest.current.dirty = true;
    const t = setTimeout(flush, SAVE_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [index, flush]);
  useEffect(() => {
    const onHide = () => flush();
    window.addEventListener('pagehide', onHide);
    return () => {
      window.removeEventListener('pagehide', onHide);
      flush(); // leaving the reader saves at once
    };
  }, [flush]);

  /* ------------------------------------------------------- preloading */
  const nextId = manifest.next_chapter_id;
  useEffect(() => {
    const w = stageWidth;
    const dpr = window.devicePixelRatio || 1;
    const want = [index, ...(strip ? [] : [index + 1])].filter((i) => i < count);
    for (const i of [...want, ...preloadWindow(index, count, 3)])
      void preloadImage(
        pickVariant(pages[i]!, strip ? w : w / (prefs.mode === 'double' ? 2 : 1), dpr),
      );
    // First pages of the next chapter, once the end is near.
    if (nextId && index >= count - 4) {
      void qc
        .fetchQuery({
          queryKey: ['library', 'pages', nextId],
          queryFn: () => getPages(nextId),
          staleTime: 5 * 60_000,
        })
        .then((m) => {
          for (const p of m.pages.slice(0, 3)) void preloadImage(pickVariant(p, w, dpr));
        })
        .catch(() => undefined); // a restricted next chapter is asked about when it is opened
    }
  }, [index, count, pages, stageWidth, strip, prefs.mode, nextId, qc]);

  /* ------------------------------------------------- chapters + marks */
  const chapterList = useQuery({
    queryKey: ['library', 'chapters', series.id, chapter.edition_id],
    queryFn: () => getChapters(series.id, chapter.edition_id),
    staleTime: 60_000,
  });
  const bookmarks = useQuery({
    queryKey: ['library', 'bookmarks', chapter.id],
    queryFn: () => progressApi.bookmarks(chapter.id),
  });
  const refreshMarks = () =>
    qc.invalidateQueries({ queryKey: ['library', 'bookmarks', chapter.id] });
  const pageIdx = Math.min(index, Math.max(0, count - 1));
  const markHere = bookmarks.data?.find((b) => b.pageIndex === pageIdx);
  const toggleBookmark = useCallback(
    async (note = '') => {
      if (markHere && !note) await progressApi.removeBookmark(chapter.id, markHere.id);
      else await progressApi.addBookmark(chapter.id, pageIdx, note);
      await refreshMarks();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [markHere, chapter.id, pageIdx],
  );

  /* ---------------------------------------------------------- keyboard */
  const setMode = (mode: ReaderMode) =>
    useReaderPrefsStore.getState().set({ mode }, { seriesId: series.id, override: true });
  const toggleFullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void rootRef.current?.requestFullscreen?.();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || isEditable(e.target) || panel) return;
      let handled = true;
      switch (e.key) {
        case 'ArrowLeft':
        case 'ArrowRight':
          step(strip ? (e.key === 'ArrowRight' ? 1 : -1) : arrowStep(e.key, prefs.direction));
          break;
        case 'ArrowDown':
        case 'PageDown':
          step(1);
          break;
        case 'ArrowUp':
        case 'PageUp':
          step(-1);
          break;
        case ' ':
          step(e.shiftKey ? -1 : 1);
          break;
        case 'Home':
          goIndex(0);
          break;
        case 'End':
          goIndex(count - 1);
          break;
        case 'f':
        case 'F':
          toggleFullscreen();
          break;
        case 'd':
        case 'D':
          setMode(prefs.mode === 'double' ? 'paged' : 'double');
          break;
        case 'm':
        case 'M':
          setPanel('settings');
          break;
        case 'b':
        case 'B':
          void toggleBookmark();
          break;
        case '+':
        case '=':
          zoom.zoomBy(1.25);
          break;
        case '-':
          zoom.zoomBy(0.8);
          break;
        case '0':
          zoom.reset();
          break;
        default:
          handled = false;
      }
      if (handled) {
        e.preventDefault();
        poke();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, strip, prefs.direction, prefs.mode, panel, count, goIndex, toggleBookmark, zoom, poke]);

  /* ------------------------------------------------------------ strip */
  const stripRefs = useRef<(HTMLElement | null)[]>([]);
  const skipScroll = useRef(false);
  useEffect(() => {
    if (!strip) return;
    // Jump (slider, Home/End, bookmark, initial position) scrolls the strip; scroll-driven updates do not.
    if (skipScroll.current) {
      skipScroll.current = false;
      return;
    }
    stripRefs.current[index]?.scrollIntoView?.({ block: 'start' });
  }, [strip, index]);
  useEffect(() => {
    if (!strip || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          const i = Number((e.target as HTMLElement).dataset.index);
          skipScroll.current = true;
          setIndex(i);
        }
      },
      { root: stageRef.current, rootMargin: '-45% 0px -45% 0px' },
    );
    for (const el of stripRefs.current) if (el) io.observe(el);
    return () => io.disconnect();
  }, [strip, count]);

  /* --------------------------------------------------------- stage tap */
  const onStageClick = (e: React.MouseEvent) => {
    if (zoom.wasDrag() || zoom.zoomed) return;
    const r = e.currentTarget.getBoundingClientRect();
    const dir =
      prefs.tapZones && !strip ? tapStep(e.clientX - r.left, r.width, prefs.direction) : 0;
    if (dir) step(dir);
    else setChromeOn((v) => !v);
  };

  /* ------------------------------------------------------------- view */
  const title = `${series.title} · Chapter ${chapter.number}${chapter.title ? `: ${chapter.title}` : ''}`;
  const current =
    spreads[index >= count ? spreads.length - 1 : spreadIndexOf(spreads, index)] ?? [];
  const rtl = !strip && prefs.direction === 'rtl';
  const showEnd = index >= count;

  return (
    <div
      ref={rootRef}
      className="lib-reader"
      data-bg={prefs.background}
      data-chrome={chromeVisible ? 'on' : 'off'}
      data-smooth={prefs.smoothing ? 'on' : 'off'}
      data-fit={prefs.fit}
      onMouseMove={poke}
    >
      <header className="lib-reader-top" aria-label="Reader">
        <Link
          to="/library/s/$seriesId"
          params={{ seriesId: series.id }}
          className="lib-icon-btn"
          aria-label={`Back to ${series.title}`}
        >
          <Icon name="arrow-left" size={20} />
        </Link>
        <h1 className="lib-reader-title" title={title}>
          {title}
        </h1>
        <ChapterSelect
          chapters={chapterList.data?.chapters ?? []}
          current={chapter}
          onPick={(id) => leaveTo(id, false)}
        />
        <button
          type="button"
          className="lib-icon-btn"
          aria-label={markHere ? 'Remove bookmark from this page' : 'Bookmark this page'}
          aria-pressed={!!markHere}
          onClick={() => void toggleBookmark()}
        >
          <Icon name="bookmark" size={20} />
        </button>
        <button
          type="button"
          className="lib-icon-btn"
          aria-label="Bookmarks"
          onClick={() => setPanel('bookmarks')}
        >
          <Icon name="list" size={20} />
        </button>
        <button
          type="button"
          className="lib-icon-btn"
          aria-label="Reader settings"
          onClick={() => setPanel('settings')}
        >
          <Icon name="settings" size={20} />
        </button>
      </header>

      <main
        ref={stageRef}
        className={strip ? 'lib-stage lib-stage-strip' : 'lib-stage'}
        data-mode={prefs.mode}
        aria-label="Chapter pages"
        onClick={strip ? () => setChromeOn((v) => !v) : onStageClick}
        {...(strip ? {} : zoom.bind)}
      >
        {strip ? (
          <div className="lib-strip" style={{ gap: prefs.gap }}>
            {pages.map((p, i) => (
              <div
                key={p.index}
                ref={(el) => {
                  stripRefs.current[i] = el;
                }}
                data-index={i}
                className="lib-strip-item"
              >
                <PageImage
                  page={p}
                  cssWidth={Math.min(stageWidth, p.width)}
                  active={Math.abs(i - index) <= 4}
                  alt={`Page ${i + 1}`}
                />
              </div>
            ))}
            <EndCard
              chapter={chapter}
              next={manifest.next_chapter_id}
              seriesId={series.id}
              onNext={leaveTo}
            />
          </div>
        ) : showEnd ? (
          <EndCard
            chapter={chapter}
            next={manifest.next_chapter_id}
            seriesId={series.id}
            onNext={leaveTo}
          />
        ) : (
          <div
            className="lib-spread"
            data-dir={prefs.direction}
            style={{
              transform: `translate(${zoom.zoom.x}px, ${zoom.zoom.y}px) scale(${zoom.zoom.scale})`,
              flexDirection: rtl ? 'row-reverse' : 'row',
            }}
          >
            {current.map((i) => (
              <PageImage
                key={pages[i]!.index}
                page={pages[i]!}
                cssWidth={stageWidth / current.length}
                alt={`Page ${i + 1}`}
              />
            ))}
          </div>
        )}
        {prefs.brightness < 100 && (
          <div
            className="lib-dim"
            style={{ opacity: 1 - prefs.brightness / 100 }}
            aria-hidden="true"
          />
        )}
      </main>

      <footer className="lib-reader-bottom">
        {scrub !== null && pages[scrub] && (
          <img
            className="lib-scrub-thumb"
            alt={`Preview of page ${scrub + 1}`}
            src={pickVariant(pages[scrub]!, 120, 1)}
          />
        )}
        <button
          type="button"
          className="lib-icon-btn"
          aria-label="Previous chapter"
          disabled={!manifest.prev_chapter_id}
          onClick={() => manifest.prev_chapter_id && leaveTo(manifest.prev_chapter_id, false)}
        >
          <Icon name="arrow-left" size={20} />
        </button>
        <div className="lib-reader-slider" dir={rtl ? 'rtl' : 'ltr'}>
          <Slider
            label="Page"
            min={1}
            max={Math.max(1, count)}
            value={pageIdx + 1}
            showValue
            formatValue={(v) => `${v} / ${count}`}
            onValueChange={(v) => {
              setScrub(v - 1);
              goIndex(v - 1);
              window.setTimeout(() => setScrub(null), 900);
            }}
          />
        </div>
        <button
          type="button"
          className="lib-icon-btn"
          aria-label="Next chapter"
          disabled={!manifest.next_chapter_id}
          onClick={() => manifest.next_chapter_id && leaveTo(manifest.next_chapter_id, false)}
        >
          <Icon name="arrow-right" size={20} />
        </button>
      </footer>

      <p className="lib-sr-only" role="status" aria-live="polite" data-testid="reader-status">
        {showEnd ? 'End of chapter' : `Page ${pageIdx + 1} of ${count}`}
      </p>

      <SettingsPanel
        open={panel === 'settings'}
        onClose={() => setPanel(null)}
        seriesId={series.id}
        prefs={prefs as ReaderPrefs}
      />
      <BookmarksPanel
        open={panel === 'bookmarks'}
        onClose={() => setPanel(null)}
        bookmarks={bookmarks.data ?? []}
        currentPage={pageIdx}
        onAdd={(note) => void toggleBookmark(note || 'Bookmark')}
        onRemove={(id) => void progressApi.removeBookmark(chapter.id, id).then(refreshMarks)}
        onJump={goIndex}
      />
    </div>
  );
}

function ChapterSelect({
  chapters,
  current,
  onPick,
}: {
  chapters: Chapter[];
  current: Chapter;
  onPick: (id: string) => void;
}) {
  const sorted = useMemo(() => [...chapters].sort((a, b) => a.number - b.number), [chapters]);
  if (sorted.length < 2) return null;
  return (
    <label className="lib-chapter-select">
      <span className="lib-sr-only">Chapter</span>
      <select value={current.id} onChange={(e) => onPick(e.target.value)}>
        {sorted.map((c) => (
          <option key={c.id} value={c.id}>
            Chapter {c.number}
            {c.title ? `: ${c.title}` : ''}
          </option>
        ))}
      </select>
    </label>
  );
}

function EndCard({
  chapter,
  next,
  seriesId,
  onNext,
}: {
  chapter: Chapter;
  next?: string | null;
  seriesId: string;
  onNext: (id: string, last: boolean) => void;
}) {
  return (
    <section className="lib-endcard" aria-label="End of chapter">
      <p className="lib-eyebrow">End of chapter {chapter.number}</p>
      {next ? (
        <button type="button" className="lib-btn lib-btn-gold" onClick={() => onNext(next, false)}>
          Next chapter
          <Icon name="arrow-right" size={16} />
        </button>
      ) : (
        <p>You are all caught up.</p>
      )}
      <Link className="lib-btn" to="/library/s/$seriesId" params={{ seriesId }}>
        Back to the series
      </Link>
      <p className="lib-muted">Rating and comments for chapters arrive with the social features.</p>
    </section>
  );
}
