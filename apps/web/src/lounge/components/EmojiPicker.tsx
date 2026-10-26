import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useGuilds } from '../data';
import './emojiPicker.css';
import { EMOJI_CATEGORIES, TONEABLE } from './emoji';
import {
  frequent,
  loadEmojiPrefs,
  recordPick,
  saveEmojiPrefs,
  toggleFavorite,
  withTone,
  type EmojiPrefs,
} from './emojiPrefs';

/** A custom emoji of a server (`GET /guilds/{id}/emojis`). */
export interface GuildEmoji {
  id: string;
  name: string;
  animated: boolean;
  available: boolean;
  /** Role ids that may use it; empty = everyone. */
  roles: string[];
}

const lists = new Map<string, Promise<GuildEmoji[]>>();
/** Custom emoji of a guild, fetched once per session. */
export function loadGuildEmojis(guildId: string): Promise<GuildEmoji[]> {
  let p = lists.get(guildId);
  if (!p) {
    p = apiFetch<GuildEmoji[]>(`/guilds/${guildId}/emojis`, { retries: 0 }).catch((e) => {
      lists.delete(guildId);
      throw e;
    });
    lists.set(guildId, p);
  }
  return p;
}
/** Test hook. */
export const resetGuildEmojis = () => lists.clear();

const urls = new Map<string, Promise<string | null>>();
function emojiUrl(guildId: string, id: string): Promise<string | null> {
  const key = `${guildId}:${id}`;
  let p = urls.get(key);
  if (!p) {
    p = apiFetch<{ url: string }>(`/guilds/${guildId}/emojis/${id}/url?size=64`, { retries: 0 })
      .then((r) => r.url)
      .catch(() => {
        urls.delete(key);
        return null;
      });
    urls.set(key, p);
  }
  return p;
}

/** The message-text form of a custom emoji. */
export const customEmojiText = (e: Pick<GuildEmoji, 'id' | 'name' | 'animated'>): string =>
  `<${e.animated ? 'a' : ''}:${e.name}:${e.id}>`;

function CustomTile({
  guildId,
  emoji,
  onPick,
  onHover,
}: {
  guildId: string;
  emoji: GuildEmoji;
  onPick: (e: GuildEmoji) => void;
  onHover: (label: string | null) => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void emojiUrl(guildId, emoji.id).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [guildId, emoji.id]);
  return (
    <button
      type="button"
      className="lng-emoji-btn"
      aria-label={emoji.name}
      onClick={() => onPick(emoji)}
      onMouseEnter={() => onHover(`:${emoji.name}:`)}
      onMouseLeave={() => onHover(null)}
    >
      {url ? <img src={url} alt="" width={24} height={24} loading="lazy" /> : null}
    </button>
  );
}

const WAVE = '\u{1F44B}';
const TONE_SWATCH = [WAVE, ...[1, 2, 3, 4, 5].map((n) => withTone(WAVE, n))];

interface CatTab {
  id: string;
  label: string;
  glyph: string;
  icon?: string | null;
}

interface Entry {
  name: string;
  ch: string;
  toneable: boolean;
}

/**
 * Emoji picker (LNG-EXP-007): search, frequently used, favorites (right-click an emoji to
 * toggle), this server's custom emoji, a skin tone selector and a hover preview footer.
 * Custom emoji only appear where the caller can insert them (`guildId` and `onPickCustom`).
 */
export function EmojiPicker({
  onPick,
  guildId,
  onPickCustom,
}: {
  onPick: (emoji: string) => void;
  guildId?: string | undefined;
  onPickCustom?: ((emoji: GuildEmoji) => void) | undefined;
}) {
  const t = useT();
  const [q, setQ] = useState('');
  const [prefs, setPrefs] = useState<EmojiPrefs>(() => loadEmojiPrefs());
  const [custom, setCustom] = useState<GuildEmoji[]>([]);
  const [hover, setHover] = useState<{ label: string; glyph: string | null } | null>(null);
  const [active, setActive] = useState<string>('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const guild = useGuilds().find((g) => g.id === guildId);
  const update = (next: EmojiPrefs) => {
    setPrefs(next);
    saveEmojiPrefs(next);
  };
  useEffect(() => {
    if (!guildId || !onPickCustom) return;
    let live = true;
    loadGuildEmojis(guildId)
      .then((l) => live && setCustom(l.filter((e) => e.available)))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [guildId, onPickCustom]);

  const needle = q.trim().toLowerCase();
  const byChar = useMemo(() => {
    const map = new Map<string, Entry>();
    for (const c of EMOJI_CATEGORIES)
      for (const [name, ch] of c.emoji) map.set(ch, { name, ch, toneable: TONEABLE.has(name) });
    return map;
  }, []);
  const toned = (e: Entry) => (e.toneable ? withTone(e.ch, prefs.tone) : e.ch);
  // Counts and favorites are keyed by the base character, so changing the tone keeps them.
  const pick = (e: Entry) => {
    update(recordPick(prefs, e.ch));
    onPick(toned(e));
  };

  const groups = EMOJI_CATEGORIES.map((c) => ({
    name: c.name,
    emoji: needle ? c.emoji.filter(([n]) => n.includes(needle)) : c.emoji,
  })).filter((c) => c.emoji.length);
  const customShown = needle ? custom.filter((e) => e.name.toLowerCase().includes(needle)) : custom;
  const entries = (keys: string[]): Entry[] =>
    keys.map((k) => byChar.get(k)).filter((e): e is Entry => !!e);
  const quick = needle ? [] : entries(frequent(prefs));
  const favs = needle ? [] : entries(prefs.favorites);

  // The category sidebar lists exactly the sections that are on screen (search narrows both).
  const catTabs: CatTab[] = [
    ...(favs.length ? [{ id: 'fav', label: t('lounge.emojiPicker.favorites'), glyph: '⭐' }] : []),
    ...(quick.length ? [{ id: 'freq', label: t('lounge.emojiPicker.frequent'), glyph: '🕒' }] : []),
    ...(guildId && onPickCustom && customShown.length
      ? [
          {
            id: 'server',
            label: t('lounge.emojiPicker.thisServer'),
            glyph: (guild?.name ?? '?').slice(0, 1).toUpperCase(),
            icon: guild?.iconUrl ?? null,
          },
        ]
      : []),
    ...groups.map((g) => ({ id: `cat-${g.name}`, label: g.name, glyph: g.emoji[0]![1] })),
  ];
  const current = catTabs.some((c) => c.id === active) ? active : (catTabs[0]?.id ?? '');
  const sectionTop = (id: string): number | null => {
    const box = scrollRef.current;
    const el = box?.querySelector<HTMLElement>(`[data-cat="${id}"]`);
    if (!box || !el) return null;
    return el.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop;
  };
  const jump = (id: string) => {
    const box = scrollRef.current;
    const top = sectionTop(id);
    if (box && top !== null) box.scrollTop = top;
    setActive(id);
  };
  // Scroll sync: the sidebar highlights the last section whose top has passed the scroll edge.
  const onScroll = () => {
    const box = scrollRef.current;
    if (!box) return;
    let found = catTabs[0]?.id ?? '';
    for (const c of catTabs) {
      const top = sectionTop(c.id);
      if (top !== null && top <= box.scrollTop + 4) found = c.id;
    }
    setActive(found);
  };
  const tabRefs = useRef(new Map<string, HTMLButtonElement>());
  const onTabKey = (ev: KeyboardEvent, i: number) => {
    const last = catTabs.length - 1;
    const next =
      ev.key === 'ArrowDown' || ev.key === 'ArrowRight'
        ? Math.min(i + 1, last)
        : ev.key === 'ArrowUp' || ev.key === 'ArrowLeft'
          ? Math.max(i - 1, 0)
          : ev.key === 'Home'
            ? 0
            : ev.key === 'End'
              ? last
              : -1;
    if (next < 0) return;
    ev.preventDefault();
    const id = catTabs[next]!.id;
    tabRefs.current.get(id)?.focus();
    jump(id);
  };

  const button = (e: Entry, keyPrefix: string) => (
    <button
      key={`${keyPrefix}${e.name}`}
      type="button"
      className="lng-emoji-btn"
      aria-label={e.name.replace(/_/g, ' ')}
      data-favorite={prefs.favorites.includes(e.ch) || undefined}
      onClick={() => pick(e)}
      onContextMenu={(ev) => {
        ev.preventDefault();
        update(toggleFavorite(prefs, e.ch));
      }}
      onMouseEnter={() => setHover({ label: `:${e.name}:`, glyph: toned(e) })}
      onMouseLeave={() => setHover(null)}
      onFocus={() => setHover({ label: `:${e.name}:`, glyph: toned(e) })}
    >
      {toned(e)}
    </button>
  );

  return (
    <div className="lng-emoji">
      <div className="lng-emoji-top">
        <input
          type="search"
          className="lng-emoji-search"
          placeholder={t('lounge.emojiPicker.placeholder')}
          aria-label={t('lounge.emojiPicker.search')}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          data-autofocus
        />
        <select
          className="lng-emoji-tone"
          aria-label={t('lounge.emojiPicker.skinTone')}
          value={prefs.tone}
          onChange={(e) => update({ ...prefs, tone: Number(e.target.value) })}
        >
          {TONE_SWATCH.map((sw, i) => (
            <option key={sw} value={i}>
              {sw}
            </option>
          ))}
        </select>
      </div>
      <div className="lng-emoji-body">
        {catTabs.length > 0 && (
          <div
            className="lng-emoji-cats"
            role="toolbar"
            aria-orientation="vertical"
            aria-label={t('lounge.emojiPicker.categories')}
          >
            {catTabs.map((c, i) => (
              <button
                key={c.id}
                type="button"
                className="lng-emoji-cat"
                ref={(el) => {
                  if (el) tabRefs.current.set(c.id, el);
                  else tabRefs.current.delete(c.id);
                }}
                aria-label={c.label}
                title={c.label}
                aria-pressed={c.id === current}
                tabIndex={c.id === current ? 0 : -1}
                onClick={() => jump(c.id)}
                onKeyDown={(ev) => onTabKey(ev, i)}
              >
                {c.icon ? <img src={c.icon} alt="" width={20} height={20} /> : c.glyph}
              </button>
            ))}
          </div>
        )}
        <div className="lng-emoji-scroll" ref={scrollRef} onScroll={onScroll}>
          {favs.length > 0 && (
            <section aria-label={t('lounge.emojiPicker.favorites')} data-cat="fav">
              <h3 className="lng-emoji-h">{t('lounge.emojiPicker.favorites')}</h3>
              <div className="lng-emoji-grid">{favs.map((e) => button(e, 'fav-'))}</div>
            </section>
          )}
          {quick.length > 0 && (
            <section aria-label={t('lounge.emojiPicker.frequent')} data-cat="freq">
              <h3 className="lng-emoji-h">{t('lounge.emojiPicker.frequent')}</h3>
              <div className="lng-emoji-grid">{quick.map((e) => button(e, 'freq-'))}</div>
            </section>
          )}
          {guildId && onPickCustom && customShown.length > 0 && (
            <section aria-label={t('lounge.emojiPicker.thisServer')} data-cat="server">
              <h3 className="lng-emoji-h">{t('lounge.emojiPicker.thisServer')}</h3>
              <div className="lng-emoji-grid">
                {customShown.map((e) => (
                  <CustomTile
                    key={e.id}
                    guildId={guildId}
                    emoji={e}
                    onPick={onPickCustom}
                    onHover={(label) => setHover(label ? { label, glyph: null } : null)}
                  />
                ))}
              </div>
            </section>
          )}
          {groups.length === 0 && customShown.length === 0 && (
            <p className="lng-muted">{t('lounge.emojiPicker.none')}</p>
          )}
          {groups.map((g) => (
            <section key={g.name} aria-label={g.name} data-cat={`cat-${g.name}`}>
              <h3 className="lng-emoji-h">{g.name}</h3>
              <div className="lng-emoji-grid">
                {g.emoji.map(([name, ch]) =>
                  button({ name, ch, toneable: TONEABLE.has(name) }, ''),
                )}
              </div>
            </section>
          ))}
        </div>
      </div>
      <div className="lng-emoji-footer" aria-live="polite">
        {hover ? (
          <>
            {hover.glyph && <span className="lng-emoji-preview">{hover.glyph}</span>}
            <span>{hover.label}</span>
          </>
        ) : (
          <span className="lng-muted">{t('lounge.emojiPicker.hint')}</span>
        )}
      </div>
    </div>
  );
}
