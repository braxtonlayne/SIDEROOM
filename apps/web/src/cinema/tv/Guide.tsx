// The Live TV guide page, `/cinema/tv` (TV-UI-001..010; spec cinema-live-tv section 2). The page (and only this
// page) wears the broadcast theme: a nested ThemeProvider with section "live-tv" resolves `sideroom-broadcast`.
import { EmptyState, Spinner, ThemeProvider, useMediaQuery, MOBILE_QUERY } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getMe as getAdminMe } from '../../admin/api';
import { hasPerm, parsePerms } from '../../admin/permissions';
import { useAppliedTheme } from '../../settings/user/useAppliedTheme';
import { getChannelNow, getGuide, isNotAvailable, tvKeys } from './api';
import { useFavourites } from './favourites';
import { GuideGrid, type TzMode } from './GuideGrid';
import { MobileAgenda } from './MobileAgenda';
import {
  FILTERS,
  SLOT_MS,
  WINDOW_MS,
  addDays,
  dayLabel,
  filterChannels,
  lineupName,
  nowAndNext,
  startOfDay,
  type TvFilter,
} from './model';
import { NowPanel } from './NowPanel';
import { ProgramPopover } from './ProgramPopover';
import type { TvBlock } from './types';
import './tv.css';

export const DEFAULT_FOOTER = 'Channels are assembled from the SIDE ROOM library.';
const CHAN_COL = 306;

/** Wall clock that follows the server's (`offset` = server now minus client now) and ticks every `every` ms. */
function useClock(offset: number, every = 20_000) {
  const [, tick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => tick((n) => n + 1), every);
    return () => clearInterval(id);
  }, [every]);
  return Date.now() + offset;
}

function useCanManageTv() {
  const q = useQuery({
    queryKey: ['cinema', 'tv', 'admin-me'],
    queryFn: getAdminMe,
    staleTime: 5 * 60_000,
    retry: false,
  });
  return q.data ? hasPerm(parsePerms(q.data.platform_permissions), 'MANAGE_TV') : false;
}

export function TvGuidePage() {
  const theme = useAppliedTheme();
  return (
    // Broadcast theme for this page only. `perSection` keeps a viewer's own per-section choice; the global
    // theme choice does not apply here (the teletext layer needs its tokens).
    <ThemeProvider
      section="live-tv"
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={{ background: 'transparent' }}
      className="tv-root"
    >
      <GuideContent />
    </ThemeProvider>
  );
}

function GuideContent() {
  const mobile = useMediaQuery(MOBILE_QUERY);
  const canManage = useCanManageTv();
  const fav = useFavourites();
  const [dayStart, setDayStart] = useState(() => startOfDay(Date.now()));
  const dayEnd = addDays(dayStart, 1);
  const [filter, setFilter] = useState<TvFilter>('all');
  const [tzMode, setTzMode] = useState<TzMode>('local');
  const [picked, setPicked] = useState<string | null>(null);
  const [open, setOpen] = useState<{ block: TvBlock; anchor: HTMLElement | null } | null>(null);
  const [viewStart, setViewStart] = useState<number | null>(null);
  const [gridW, setGridW] = useState<number | null>(null);

  const guide = useQuery({
    queryKey: tvKeys.guide(dayStart, dayEnd),
    queryFn: () => getGuide(dayStart, dayEnd),
    staleTime: 60_000,
    refetchInterval: 5 * 60_000,
    retry: false,
  });
  // Follow the server clock (the guide carries its now) from the first render with data.
  const offset = guide.data ? guide.data.now_ms - guide.dataUpdatedAt : 0;
  const nowMs = useClock(offset);

  const channels = useMemo(
    () =>
      filterChannels(guide.data?.channels ?? [], guide.data?.blocks ?? {}, filter, fav.favourites),
    [guide.data, filter, fav.favourites],
  );
  const selected =
    channels.find((c) => c.id === picked) ??
    channels.find((c) => nowAndNext(guide.data?.blocks[c.id], nowMs).now) ??
    channels[0] ??
    null;

  const nowQ = useQuery({
    queryKey: tvKeys.now(selected?.id ?? ''),
    queryFn: () => getChannelNow(selected!.id),
    enabled: !!selected,
    refetchInterval: 60_000,
    retry: false,
    staleTime: 30_000,
  });
  const fromGuide = selected ? nowAndNext(guide.data?.blocks[selected.id], nowMs) : null;
  const now = nowQ.data?.now ?? fromGuide?.now ?? null;
  const next = nowQ.data?.next ?? fromGuide?.next ?? null;
  const healthy = nowQ.data ? nowQ.data.healthy : nowQ.isError ? false : null;

  /* ---- scrolling ---- */
  const scroller = useRef<HTMLDivElement | null>(null);
  const setScroller = useCallback((el: HTMLDivElement | null) => {
    scroller.current = el;
  }, []);
  const slotPx = Math.min(340, Math.max(150, Math.floor(((gridW ?? 0) - CHAN_COL) / 4)) || 150);
  const reduced = useMediaQuery('(prefers-reduced-motion: reduce)');
  const scrollTo = useCallback(
    (left: number) => scroller.current?.scrollTo?.({ left, behavior: reduced ? 'auto' : 'smooth' }),
    [reduced],
  );
  const jumpToNow = useCallback(() => {
    const today = startOfDay(nowMs);
    if (today !== dayStart) {
      setDayStart(today);
      return;
    }
    scrollTo(Math.max(0, ((nowMs - dayStart) / SLOT_MS) * slotPx - slotPx * 0.5));
  }, [nowMs, dayStart, slotPx, scrollTo]);

  const wrapRef = useRef<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const measure = () => {
      const cs = getComputedStyle(el);
      setGridW(
        el.clientWidth - parseFloat(cs.paddingLeft || '0') - parseFloat(cs.paddingRight || '0'),
      );
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [mobile]);

  // First paint of a day: today starts at the now line, other days at 18:00 (the evening lineup).
  const placed = useRef<number | null>(null);
  useEffect(() => {
    const el = scroller.current;
    if (!el || !guide.data || gridW === null || placed.current === dayStart) return;
    placed.current = dayStart;
    const target =
      startOfDay(nowMs) === dayStart
        ? ((nowMs - dayStart) / SLOT_MS) * slotPx - slotPx * 0.5
        : ((18 * 60 * 60_000) / SLOT_MS) * slotPx;
    el.scrollLeft = Math.max(0, target);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guide.data, dayStart, gridW]);

  // A resize changes the column width: keep the same time at the left edge.
  const lastSlot = useRef(slotPx);
  useEffect(() => {
    const el = scroller.current;
    if (el && lastSlot.current !== slotPx)
      el.scrollLeft = (el.scrollLeft / lastSlot.current) * slotPx;
    lastSlot.current = slotPx;
  }, [slotPx]);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onScroll = () => setViewStart(dayStart + (el.scrollLeft / slotPx) * SLOT_MS);
    el.addEventListener('scroll', onScroll, { passive: true });
    onScroll();
    return () => el.removeEventListener('scroll', onScroll);
  }, [dayStart, slotPx, guide.data, mobile]);

  // `n` jumps to now from anywhere on the page (the grid handles it too and marks the event handled).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target;
      const typing =
        t instanceof HTMLElement &&
        (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if (e.defaultPrevented || typing || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === 'n' || e.key === 'N') jumpToNow();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [jumpToNow]);

  const today = startOfDay(nowMs);
  const canPrevDay = dayStart > addDays(today, -1);
  const canNextDay = dayStart < addDays(today, 7);

  const popChannel = open
    ? (guide.data?.channels.find((c) => c.id === open.block.channel_id) ?? null)
    : null;
  const closePop = useCallback(() => setOpen(null), []);

  /* ---- body ---- */
  let body;
  if (guide.isPending) {
    body = (
      <div className="tv-state">
        <Spinner label="Loading the guide" />
      </div>
    );
  } else if (guide.isError) {
    body = isNotAvailable(guide.error) ? (
      <EmptyState
        title="Live TV is not available yet"
        description="This server has not switched on the television room. Check back later."
      />
    ) : (
      <div role="alert" className="tv-state">
        <p>We could not load the guide.</p>
        <button type="button" className="tv-btn" onClick={() => void guide.refetch()}>
          Try again
        </button>
      </div>
    );
  } else if (guide.data.channels.length === 0) {
    body = (
      <EmptyState
        title="No channels yet"
        description={
          canManage
            ? 'Create the first channel to fill the guide.'
            : 'The operators have not set up any channels yet.'
        }
      />
    );
  } else if (channels.length === 0) {
    body = (
      <EmptyState
        title={filter === 'favorites' ? 'No favorites yet' : 'No channels here'}
        description={
          filter === 'favorites'
            ? 'Star a channel in the guide to keep it in this list.'
            : 'Try another filter.'
        }
      />
    );
  } else if (mobile) {
    body = (
      <MobileAgenda
        channels={channels}
        blocks={guide.data.blocks}
        nowMs={nowMs}
        dayStart={dayStart}
        dayEnd={dayEnd}
        tzLocal={tzMode === 'local'}
        favourites={fav.favourites}
        onToggleFavourite={fav.toggle}
        onOpen={(block) => setOpen({ block, anchor: null })}
      />
    );
  } else {
    body = (
      <GuideGrid
        ref={setScroller}
        channels={channels}
        blocks={guide.data.blocks}
        nowMs={nowMs}
        dayStart={dayStart}
        dayEnd={dayEnd}
        slotPx={slotPx}
        tzMode={tzMode}
        favourites={fav.favourites}
        selectedChannelId={selected?.id ?? null}
        onToggleFavourite={fav.toggle}
        onSelectChannel={setPicked}
        onOpen={(block, anchor) => setOpen({ block, anchor })}
        onJumpToNow={jumpToNow}
      />
    );
  }

  return (
    <div className="tv-page cin-page" ref={wrapRef} data-mobile={mobile || undefined}>
      <header className="tv-head-block">
        <div>
          <p className="tv-eyebrow">
            <span aria-hidden="true" className="tv-rule" />
            THE TELEVISION ROOM
          </p>
          <h1 className="tv-h1">A familiar frequency.</h1>
        </div>
        {canManage && (
          <a className="tv-btn" href="/admin/cinema-live-tv?new=1">
            + Create channel
          </a>
        )}
      </header>

      {selected && guide.data && (
        <NowPanel
          channel={selected}
          now={now}
          next={next}
          healthy={healthy}
          canManage={canManage}
          onDetails={(block) => setOpen({ block, anchor: null })}
        />
      )}

      <div className="tv-controls">
        <div role="group" aria-label="Channel filter" className="tv-chips">
          {FILTERS.map((f) => (
            <button
              key={f.value}
              type="button"
              className="tv-chip"
              aria-pressed={filter === f.value}
              onClick={() => setFilter(f.value)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <div className="tv-nav" role="group" aria-label="Guide time">
          <button
            type="button"
            className="tv-chip"
            aria-pressed={tzMode === 'channel'}
            title="Show programme times in each channel's own time zone"
            onClick={() => setTzMode((m) => (m === 'local' ? 'channel' : 'local'))}
          >
            Channel time
          </button>
          <button
            type="button"
            className="tv-nav-btn"
            aria-label="Previous day"
            disabled={!canPrevDay}
            onClick={() => setDayStart(addDays(dayStart, -1))}
          >
            ‹
          </button>
          <span className="tv-nav-day">{dayLabel(dayStart, nowMs)}</span>
          <button
            type="button"
            className="tv-nav-btn"
            aria-label="Next day"
            disabled={!canNextDay}
            onClick={() => setDayStart(addDays(dayStart, 1))}
          >
            ›
          </button>
          {!mobile && (
            <>
              <button
                type="button"
                className="tv-nav-btn"
                aria-label="Earlier"
                onClick={() =>
                  scrollTo(
                    Math.max(
                      0,
                      (scroller.current?.scrollLeft ?? 0) - (WINDOW_MS / SLOT_MS) * slotPx,
                    ),
                  )
                }
              >
                ←
              </button>
              <span className="tv-nav-lineup" aria-live="polite">
                {lineupName(viewStart ?? nowMs)}
              </span>
              <button
                type="button"
                className="tv-nav-btn"
                aria-label="Later"
                onClick={() =>
                  scrollTo((scroller.current?.scrollLeft ?? 0) + (WINDOW_MS / SLOT_MS) * slotPx)
                }
              >
                →
              </button>
            </>
          )}
          <button type="button" className="tv-chip" onClick={jumpToNow} aria-keyshortcuts="n">
            Now
          </button>
        </div>
      </div>
      {fav.failed && (
        <p role="alert" className="tv-pop-err">
          Could not save your favorites. Try again.
        </p>
      )}

      {body}

      <footer className="tv-footer">
        <span className="tv-legend">
          <span aria-hidden="true" className="tv-legend-dot" /> Current time
        </span>
        <span className="tv-footnote">{guide.data?.footer_note || DEFAULT_FOOTER}</span>
      </footer>

      {open && popChannel && guide.data && (
        <ProgramPopover
          block={open.block}
          channel={popChannel}
          anchor={mobile ? null : open.anchor}
          nowMs={nowMs}
          tz={tzMode === 'channel' ? popChannel.timezone : null}
          allBlocks={guide.data.blocks}
          channels={guide.data.channels}
          onClose={closePop}
        />
      )}
    </div>
  );
}
