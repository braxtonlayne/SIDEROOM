// EPUB reader (LIB-RDR-010): foliate-js renders the book (loaded on demand); this component adds the chrome,
// the table of contents, the reading settings and progress. Progress is the EPUB CFI plus the overall fraction,
// saved through the ordinary chapter-progress route (page_index is the fraction in thousandths).
import { Drawer, EmptyState, Icon, SegmentedControl, Slider, Spinner } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { progressApi } from '../progress';
import { loadFoliate, type FoliateView, type RelocateDetail, type TocItem } from './foliate';
import { epubCss, themeColors, useEpubSettings, type EpubFlow, type EpubTheme } from './settings';

const SAVE_DEBOUNCE_MS = 2000;
/** Progress is stored as thousandths of the book, since an EPUB has no pages. */
export const STEPS = 1000;

type State = 'loading' | 'ready' | 'failed';

export function EpubReader({
  url,
  title,
  seriesId,
  initialCfi,
  chapter,
}: {
  url: string;
  title: string;
  seriesId: string;
  initialCfi?: string | null;
  chapter?: {
    id: string;
    number: number;
    seriesTitle: string;
    coverUrl?: string | null | undefined;
  };
}) {
  const t = useT();
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<FoliateView | null>(null);
  const [state, setState] = useState<State>('loading');
  const [toc, setToc] = useState<TocItem[]>([]);
  const [where, setWhere] = useState<{ fraction: number; label: string }>({
    fraction: 0,
    label: '',
  });
  const [panel, setPanel] = useState<null | 'toc' | 'settings'>(null);
  const { settings, set } = useEpubSettings();
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  const apply = useCallback((v: FoliateView, s = settingsRef.current) => {
    v.renderer?.setAttribute?.('flow', s.flow);
    v.renderer?.setAttribute?.('margin', `${s.margin}%`);
    v.renderer?.setAttribute?.('gap', '5%');
    v.renderer?.setStyles?.(epubCss(s));
  }, []);

  const latest = useRef<{ cfi: string | null; fraction: number; dirty: boolean }>({
    cfi: null,
    fraction: 0,
    dirty: false,
  });

  // Open the book.
  useEffect(() => {
    let live = true;
    let el: FoliateView | null = null;
    (async () => {
      try {
        await loadFoliate();
        const res = await fetch(url);
        if (!res.ok) throw new Error(`book ${res.status}`);
        const file = new File([await res.blob()], 'book.epub', { type: 'application/epub+zip' });
        if (!live || !host.current) return;
        el = document.createElement('foliate-view') as FoliateView;
        el.className = 'lib-epub-view';
        el.addEventListener('relocate', (e) => {
          const d = (e as CustomEvent<RelocateDetail>).detail;
          const fraction = Math.min(1, Math.max(0, d.fraction ?? 0));
          setWhere({ fraction, label: d.tocItem?.label ?? '' });
          latest.current = { cfi: d.cfi ?? latest.current.cfi, fraction, dirty: true };
        });
        host.current.appendChild(el);
        view.current = el;
        await el.open(file);
        apply(el);
        await el.init({ lastLocation: initialCfi ?? null, showTextStart: !initialCfi });
        if (!live) return;
        setToc(el.book?.toc ?? []);
        setState('ready');
      } catch {
        if (live) setState('failed');
      }
    })();
    return () => {
      live = false;
      el?.close?.();
      el?.remove();
      view.current = null;
    };
    // The book is opened once per URL; settings are applied by the next effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url]);

  // Settings changes restyle the open book.
  useEffect(() => {
    if (state === 'ready' && view.current) apply(view.current, settings);
  }, [settings, state, apply]);

  // Progress: debounced, and when the tab is hidden or the reader closes.
  useEffect(() => {
    if (!chapter) return;
    const flush = () => {
      const l = latest.current;
      if (!l.dirty) return;
      l.dirty = false;
      void progressApi
        .save({
          chapterId: chapter.id,
          seriesId,
          seriesTitle: chapter.seriesTitle,
          coverUrl: chapter.coverUrl ?? null,
          chapterNumber: chapter.number,
          pageCount: STEPS,
          pageIndex: Math.min(STEPS - 1, Math.round(l.fraction * STEPS)),
          percent: Math.round(l.fraction * 100),
          completed: l.fraction >= 0.99,
          cfi: l.cfi,
        })
        .catch(() => undefined);
    };
    const timer = window.setInterval(flush, SAVE_DEBOUNCE_MS);
    window.addEventListener('pagehide', flush);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('pagehide', flush);
      flush();
    };
  }, [chapter, seriesId]);

  // Arrow keys turn the page.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        panel ||
        (e.target instanceof HTMLElement && /^(INPUT|SELECT|TEXTAREA)$/.test(e.target.tagName))
      )
        return;
      if (e.key === 'ArrowRight' || e.key === 'PageDown') void view.current?.next();
      else if (e.key === 'ArrowLeft' || e.key === 'PageUp') void view.current?.prev();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [panel]);

  const back = (
    <Link to="/library/s/$seriesId" params={{ seriesId }} className="lib-btn">
      {t('library.pdf.backToSeries')}
    </Link>
  );
  if (state === 'failed')
    return (
      <div className="lib-reader lib-reader-msg" data-bg="black">
        <EmptyState
          title={t('library.epub.failedTitle')}
          description={t('library.epub.failedDescription')}
          action={back}
        />
      </div>
    );
  const c = themeColors(settings.theme);
  return (
    <div className="lib-reader lib-epub" style={{ background: c.bg, color: c.fg }}>
      <header className="lib-pdf-bar">
        {back}
        <h1 className="lib-pdf-title">{title}</h1>
        <div className="lib-pdf-nav">
          <button
            type="button"
            className="lib-btn"
            aria-label={t('library.epub.toc')}
            onClick={() => setPanel('toc')}
          >
            <Icon name="list" size={16} />
          </button>
          <button
            type="button"
            className="lib-btn"
            aria-label={t('library.reader.settings')}
            onClick={() => setPanel('settings')}
          >
            <Icon name="settings" size={16} />
          </button>
        </div>
      </header>
      {state === 'loading' && (
        <div className="lib-reader-msg">
          <Spinner label={t('library.reader.opening')} />
        </div>
      )}
      <div ref={host} className="lib-epub-host" aria-label={t('library.epub.book')} />
      <footer className="lib-epub-foot">
        <button
          type="button"
          className="lib-btn"
          aria-label={t('library.pdf.previous')}
          onClick={() => void view.current?.prev()}
        >
          <Icon name="arrow-left" size={16} />
        </button>
        <span role="status" className="lib-epub-where">
          {where.label ? `${where.label} · ` : ''}
          {t('library.epub.percent', { n: Math.round(where.fraction * 100) })}
        </span>
        <button
          type="button"
          className="lib-btn"
          aria-label={t('library.pdf.next')}
          onClick={() => void view.current?.next()}
        >
          <Icon name="arrow-right" size={16} />
        </button>
      </footer>

      <Drawer
        open={panel === 'toc'}
        onClose={() => setPanel(null)}
        title={t('library.epub.toc')}
        side="right"
      >
        {toc.length === 0 ? (
          <p className="lib-muted">{t('library.epub.noToc')}</p>
        ) : (
          <TocList
            items={toc}
            onPick={(href) => {
              setPanel(null);
              void view.current?.goTo(href);
            }}
          />
        )}
      </Drawer>
      <Drawer
        open={panel === 'settings'}
        onClose={() => setPanel(null)}
        title={t('library.epub.settings')}
        side="right"
      >
        <div className="lib-settings">
          <SegmentedControl
            label={t('library.epub.theme')}
            value={settings.theme}
            onValueChange={(v) => set({ theme: v as EpubTheme })}
            options={[
              { value: 'light', label: t('library.epub.themeLight') },
              { value: 'sepia', label: t('library.panels.settings.bgSepia') },
              { value: 'dark', label: t('library.epub.themeDark') },
            ]}
          />
          <SegmentedControl
            label={t('library.epub.flow')}
            value={settings.flow}
            onValueChange={(v) => set({ flow: v as EpubFlow })}
            options={[
              { value: 'paginated', label: t('library.epub.paged') },
              { value: 'scrolled', label: t('library.epub.scrolled') },
            ]}
          />
          <SegmentedControl
            label={t('library.epub.font')}
            value={settings.font}
            onValueChange={(v) => set({ font: v as typeof settings.font })}
            options={[
              { value: '', label: t('library.epub.fontBook') },
              { value: 'serif', label: t('library.epub.fontSerif') },
              { value: 'sans-serif', label: t('library.epub.fontSans') },
            ]}
          />
          <Slider
            label={t('library.epub.size')}
            min={80}
            max={200}
            step={10}
            value={settings.size}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(size) => set({ size })}
          />
          <Slider
            label={t('library.epub.lineHeight')}
            min={1.2}
            max={2.2}
            step={0.1}
            value={settings.lineHeight}
            showValue
            formatValue={(v) => v.toFixed(1)}
            onValueChange={(lineHeight) => set({ lineHeight })}
          />
          <Slider
            label={t('library.epub.margins')}
            min={0}
            max={15}
            step={1}
            value={settings.margin}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(margin) => set({ margin })}
          />
        </div>
      </Drawer>
    </div>
  );
}

function TocList({ items, onPick }: { items: TocItem[]; onPick: (href: string) => void }) {
  return (
    <ul className="lib-epub-toc">
      {items.map((it, i) => (
        <li key={`${it.href}-${i}`}>
          <button type="button" className="lib-linklike" onClick={() => onPick(it.href)}>
            {it.label}
          </button>
          {it.subitems && it.subitems.length > 0 && <TocList items={it.subitems} onPick={onPick} />}
        </li>
      ))}
    </ul>
  );
}
