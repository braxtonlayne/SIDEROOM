import { useCallback, useEffect, useRef, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useGifAutoplay } from '../../settings/user/gifAutoplay';
import { useAnimatedSrc } from '../msg/staticFrame';

/** A GIF as the server's provider proxy returns it (`/gifs/*`, LNG-EXP-005). */
export interface Gif {
  id: string;
  title: string;
  /** Original URL: posting it as message text makes the link preview. */
  url: string;
  /** Same-origin proxied preview; the browser never contacts the provider. */
  preview_url: string;
  width: number;
  height: number;
}
interface GifPage {
  results: Gif[];
  next?: string | null;
}
interface GifCategory {
  name: string;
  search_term: string;
  image_url: string;
}

/** A result thumbnail: plays on its own only while GIF autoplay allows it (RDY-PAR-006), else on hover or focus. */
function GifThumb({ url, width }: { url: string; width: number | undefined }) {
  const autoplay = useGifAutoplay();
  const [pointed, setPointed] = useState(false);
  const src = useAnimatedSrc(url, autoplay ? 'always' : 'hover', pointed) ?? url;
  const ref = useRef<HTMLImageElement>(null);
  // Keyboard focus on the button around the thumbnail plays it too.
  useEffect(() => {
    const btn = ref.current?.parentElement;
    if (!btn) return;
    const on = () => setPointed(true);
    const off = () => setPointed(false);
    btn.addEventListener('focus', on);
    btn.addEventListener('blur', off);
    return () => {
      btn.removeEventListener('focus', on);
      btn.removeEventListener('blur', off);
    };
  }, []);
  return (
    <img
      ref={ref}
      src={src}
      alt=""
      loading="lazy"
      width={width}
      onMouseEnter={() => setPointed(true)}
      onMouseLeave={() => setPointed(false)}
    />
  );
}

const FAVORITES_KEY = 'sr.gif.favorites';
const MAX_FAVORITES = 100;

export function loadFavorites(): Gif[] {
  try {
    const raw = localStorage.getItem(FAVORITES_KEY);
    const list = raw ? (JSON.parse(raw) as unknown) : [];
    return Array.isArray(list) ? (list as Gif[]).filter((g) => g && typeof g.id === 'string') : [];
  } catch {
    return [];
  }
}

/** Add or remove `gif`; newest first, capped. Returns the new list. */
export function toggleFavorite(list: readonly Gif[], gif: Gif): Gif[] {
  const next = list.some((g) => g.id === gif.id)
    ? list.filter((g) => g.id !== gif.id)
    : [gif, ...list].slice(0, MAX_FAVORITES);
  try {
    localStorage.setItem(FAVORITES_KEY, JSON.stringify(next));
  } catch {
    /* storage unavailable: favorites last for this session only */
  }
  return next;
}

type View = { kind: 'home' } | { kind: 'search'; q: string } | { kind: 'favorites' };

/**
 * GIF tab of the expression picker (LNG-EXP-009): trending grid, featured categories, search and a
 * favorites list. Everything comes through the server proxy; without a provider the tab says so.
 */
export function GifPicker({ onPick }: { onPick: (gif: Gif) => void }) {
  const t = useT();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [view, setView] = useState<View>({ kind: 'home' });
  const [text, setText] = useState('');
  const [items, setItems] = useState<Gif[]>([]);
  const [next, setNext] = useState<string | null>(null);
  const [categories, setCategories] = useState<GifCategory[]>([]);
  const [favorites, setFavorites] = useState<Gif[]>(() => loadFavorites());
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    apiFetch<{ enabled: boolean }>('/gifs/config', { retries: 0 })
      .then((c) => live && setEnabled(c.enabled))
      .catch(() => live && setEnabled(false));
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    apiFetch<{ categories: GifCategory[] }>('/gifs/categories', { retries: 0 })
      .then((c) => live && setCategories(c.categories))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [enabled]);

  const fetchPage = useCallback(async (v: View, pos: string | null): Promise<GifPage> => {
    const params = new URLSearchParams({ limit: '24' });
    if (pos) params.set('pos', pos);
    if (v.kind === 'search') {
      params.set('q', v.q);
      return apiFetch<GifPage>(`/gifs/search?${params}`, { retries: 0 });
    }
    return apiFetch<GifPage>(`/gifs/trending?${params}`, { retries: 0 });
  }, []);

  // Load the first page whenever the view (or the provider state) changes. Typing is debounced.
  useEffect(() => {
    if (!enabled || view.kind === 'favorites') return;
    let live = true;
    const timer = setTimeout(
      () => {
        setBusy(true);
        setError(false);
        fetchPage(view, null)
          .then((p) => {
            if (!live) return;
            setItems(p.results);
            setNext(p.next ?? null);
          })
          .catch(() => live && setError(true))
          .finally(() => live && setBusy(false));
      },
      view.kind === 'search' ? 300 : 0,
    );
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [enabled, view, fetchPage]);

  const more = () => {
    if (!next || busy) return;
    setBusy(true);
    fetchPage(view, next)
      .then((p) => {
        setItems((cur) => [...cur, ...p.results]);
        setNext(p.next ?? null);
      })
      .catch(() => setError(true))
      .finally(() => setBusy(false));
  };

  const onText = (v: string) => {
    setText(v);
    const q = v.trim();
    setView(q ? { kind: 'search', q } : { kind: 'home' });
  };

  if (enabled === null)
    return <p className="lng-muted lng-expr-note">{t('lounge.expressionPicker.loading')}</p>;
  if (!enabled)
    return <p className="lng-muted lng-expr-note">{t('lounge.expressionPicker.gifsNotSetUp')}</p>;

  const shown = view.kind === 'favorites' ? favorites : items;
  const showCategories = view.kind === 'home' && categories.length > 0;
  return (
    <div className="lng-emoji lng-gif">
      <div className="lng-gif-bar">
        <input
          type="search"
          className="lng-emoji-search"
          placeholder={t('lounge.gifPicker.placeholder')}
          aria-label={t('lounge.gifPicker.search')}
          value={text}
          onChange={(e) => onText(e.target.value)}
        />
        <button
          type="button"
          className="lng-gif-fav-toggle"
          aria-pressed={view.kind === 'favorites'}
          onClick={() => {
            setText('');
            setView(view.kind === 'favorites' ? { kind: 'home' } : { kind: 'favorites' });
          }}
        >
          {t('lounge.gifPicker.favorites')}
        </button>
      </div>
      <div className="lng-emoji-scroll">
        {showCategories && (
          <div className="lng-gif-cats">
            {categories.map((c) => (
              <button
                key={c.search_term}
                type="button"
                className="lng-gif-cat"
                onClick={() => {
                  setText(c.search_term);
                  setView({ kind: 'search', q: c.search_term });
                }}
              >
                <img src={c.image_url} alt="" loading="lazy" />
                <span>{c.name}</span>
              </button>
            ))}
          </div>
        )}
        {error && <p className="lng-muted lng-expr-note">{t('lounge.gifPicker.loadFailed')}</p>}
        {!error && !busy && shown.length === 0 && (
          <p className="lng-muted lng-expr-note">
            {view.kind === 'favorites'
              ? t('lounge.gifPicker.noFavorites')
              : t('lounge.gifPicker.noResults')}
          </p>
        )}
        <div className="lng-gif-grid">
          {shown.map((g) => {
            const fav = favorites.some((f) => f.id === g.id);
            return (
              <div key={g.id} className="lng-gif-cell">
                <button
                  type="button"
                  className="lng-gif-pick"
                  aria-label={t('lounge.gifPicker.send', { title: g.title || g.id })}
                  onClick={() => onPick(g)}
                >
                  <GifThumb url={g.preview_url} width={g.width || undefined} />
                </button>
                <button
                  type="button"
                  className="lng-gif-star"
                  aria-pressed={fav}
                  aria-label={
                    fav
                      ? t('lounge.gifPicker.unfavorite', { title: g.title || g.id })
                      : t('lounge.gifPicker.favorite', { title: g.title || g.id })
                  }
                  onClick={() => setFavorites((cur) => toggleFavorite(cur, g))}
                >
                  {fav ? '★' : '☆'}
                </button>
              </div>
            );
          })}
        </div>
        {view.kind !== 'favorites' && next && (
          <button type="button" className="lng-gif-more" disabled={busy} onClick={more}>
            {t('lounge.gifPicker.more')}
          </button>
        )}
      </div>
    </div>
  );
}
