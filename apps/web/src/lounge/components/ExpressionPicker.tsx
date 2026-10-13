import { useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../../api/client';
import { stickerUrl } from '../msg/Sticker';
import { EmojiPicker } from './EmojiPicker';

export type ExpressionTab = 'gifs' | 'stickers' | 'emoji';

export interface GuildSticker {
  id: string;
  name: string;
  tags: string;
  description?: string | null;
  /** 1 PNG, 2 APNG, 3 Lottie, 4 GIF. */
  format_type: number;
  available: boolean;
}

/** Stickers per guild, fetched once per session (`GET /guilds/{id}/stickers`). */
const stickerLists = new Map<string, Promise<GuildSticker[]>>();
export function loadGuildStickers(guildId: string): Promise<GuildSticker[]> {
  let p = stickerLists.get(guildId);
  if (!p) {
    p = apiFetch<GuildSticker[]>(`/guilds/${guildId}/stickers`, { retries: 0 }).catch((e) => {
      stickerLists.delete(guildId);
      throw e;
    });
    stickerLists.set(guildId, p);
  }
  return p;
}
/** Test hook. */
export const resetGuildStickers = () => stickerLists.clear();

/** Stickers the picker offers: usable and renderable (Lottie needs a player we do not ship). */
export function pickableStickers(list: readonly GuildSticker[], query: string): GuildSticker[] {
  const q = query.trim().toLowerCase();
  return list
    .filter((s) => s.available && s.format_type !== 3)
    .filter((s) => !q || s.name.toLowerCase().includes(q) || s.tags.toLowerCase().includes(q));
}

function StickerTile({
  guildId,
  sticker,
  onPick,
}: {
  guildId: string;
  sticker: GuildSticker;
  onPick: (s: GuildSticker) => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void stickerUrl(guildId, sticker.id).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [guildId, sticker.id]);
  return (
    <button
      type="button"
      className="lng-sticker-tile"
      aria-label={`Send sticker ${sticker.name}`}
      title={sticker.name}
      onClick={() => onPick(sticker)}
    >
      {url ? <img src={url} alt="" width={80} height={80} loading="lazy" /> : <span />}
    </button>
  );
}

/** Stickers of the current server (LNG-EXP-008); other servers' stickers cannot be sent here. */
export function StickerPicker({
  guildId,
  onPick,
}: {
  guildId: string | undefined;
  onPick: (s: GuildSticker) => void;
}) {
  const [list, setList] = useState<GuildSticker[] | null>(null);
  const [error, setError] = useState(false);
  const [q, setQ] = useState('');
  useEffect(() => {
    if (!guildId) return;
    let live = true;
    loadGuildStickers(guildId)
      .then((l) => live && setList(l))
      .catch(() => live && setError(true));
    return () => {
      live = false;
    };
  }, [guildId]);
  const shown = useMemo(() => pickableStickers(list ?? [], q), [list, q]);
  if (!guildId)
    return <p className="lng-muted lng-expr-note">Stickers are available in servers.</p>;
  if (error) return <p className="lng-muted lng-expr-note">Could not load stickers.</p>;
  if (!list) return <p className="lng-muted lng-expr-note">Loading stickers…</p>;
  return (
    <div className="lng-emoji">
      <input
        type="search"
        className="lng-emoji-search"
        placeholder="Find the perfect sticker"
        aria-label="Search stickers"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      <div className="lng-emoji-scroll">
        {shown.length === 0 ? (
          <p className="lng-muted">
            {list.length === 0 ? 'This server has no stickers yet.' : 'No stickers found.'}
          </p>
        ) : (
          <div className="lng-sticker-grid">
            {shown.map((s) => (
              <StickerTile key={s.id} guildId={guildId} sticker={s} onPick={onPick} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * GIF, sticker and emoji tabs in one popover (LNG-EXP-010). The GIF tab needs the server's GIF provider
 * proxy (LNG-EXP-005), which does not exist yet, so it says so instead of showing an empty grid.
 */
export function ExpressionPicker({
  initial = 'emoji',
  guildId,
  onEmoji,
  onSticker,
}: {
  initial?: ExpressionTab;
  guildId: string | undefined;
  onEmoji: (emoji: string) => void;
  onSticker: (s: GuildSticker) => void;
}) {
  const [tab, setTab] = useState<ExpressionTab>(initial);
  const tabs: [ExpressionTab, string][] = [
    ['gifs', 'GIFs'],
    ['stickers', 'Stickers'],
    ['emoji', 'Emoji'],
  ];
  return (
    <div className="lng-expr">
      <div role="tablist" aria-label="Expressions" className="lng-expr-tabs">
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`expr-tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`expr-panel-${id}`}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`expr-panel-${tab}`} aria-labelledby={`expr-tab-${tab}`}>
        {tab === 'gifs' && (
          <p className="lng-muted lng-expr-note">
            GIFs are not set up on this server yet. An administrator has to connect a GIF provider.
          </p>
        )}
        {tab === 'stickers' && <StickerPicker guildId={guildId} onPick={onSticker} />}
        {tab === 'emoji' && <EmojiPicker onPick={onEmoji} />}
      </div>
    </div>
  );
}
