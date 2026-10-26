import { useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { stickerUrl } from '../msg/Sticker';
import { useGuilds } from '../data';
import { EmojiPicker, type GuildEmoji } from './EmojiPicker';
import { useMyExpressions } from './myExpressions';
import { GifPicker, type Gif } from './GifPicker';

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
/** A platform sticker pack (G-44): the default packs the picker shows above a server's own stickers (D-289). */
export interface StickerPack {
  id: string;
  name: string;
  description?: string | null;
  stickers: GuildSticker[];
}

/**
 * The default sticker packs for a server, fetched once per session. Empty when the server hides them
 * (`GET /guilds/{id}/default-sticker-packs`) or when the packs cannot be read: the picker never fails over them.
 */
const packLists = new Map<string, Promise<StickerPack[]>>();
export function loadDefaultPacks(guildId: string): Promise<StickerPack[]> {
  let p = packLists.get(guildId);
  if (!p) {
    p = (async () => {
      try {
        const flag = await apiFetch<{ hidden: boolean }>(
          `/guilds/${guildId}/default-sticker-packs`,
          {
            retries: 0,
          },
        );
        if (flag.hidden) return [];
        const r = await apiFetch<{ sticker_packs: StickerPack[] }>('/sticker-packs', {
          retries: 0,
        });
        return r.sticker_packs;
      } catch {
        return [];
      }
    })();
    packLists.set(guildId, p);
  }
  return p;
}
/** Forget the cached default packs of a server (its Hide default packs switch changed). */
export const resetDefaultPacks = (guildId?: string) =>
  guildId ? packLists.delete(guildId) : packLists.clear();

/** Test hook. */
export const resetGuildStickers = () => {
  stickerLists.clear();
  packLists.clear();
};

/** Stickers the picker offers: usable and renderable (Lottie needs a player we do not ship). */
export function pickableStickers(list: readonly GuildSticker[], query: string): GuildSticker[] {
  const q = query.trim().toLowerCase();
  return list
    .filter((s) => s.available && s.format_type !== 3)
    .filter((s) => !q || s.name.toLowerCase().includes(q) || s.tags.toLowerCase().includes(q));
}

export function StickerTile({
  guildId,
  sticker,
  onPick,
  greyed,
}: {
  guildId: string;
  sticker: GuildSticker;
  onPick: (s: GuildSticker) => void;
  /** The channel refuses other servers' stickers: shown, not sendable. */
  greyed?: boolean;
}) {
  const t = useT();
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
      aria-label={t('lounge.expressionPicker.sendSticker', { name: sticker.name })}
      title={sticker.name}
      aria-disabled={greyed || undefined}
      data-greyed={greyed || undefined}
      onClick={() => {
        if (!greyed) onPick(sticker);
      }}
    >
      {url ? <img src={url} alt="" width={80} height={80} loading="lazy" /> : <span />}
    </button>
  );
}

/**
 * Stickers of the current server (LNG-EXP-008), then one section per other server whose role settings let
 * the viewer use its stickers elsewhere (D-376). When the channel refuses other servers' stickers
 * (`externalAllowed` false) those sections stay, greyed, with a tooltip saying why.
 */
export function StickerPicker({
  guildId,
  onPick,
  externalAllowed = true,
}: {
  guildId: string | undefined;
  onPick: (s: GuildSticker) => void;
  /** The target channel grants USE_EXTERNAL_STICKERS (always true in a DM). */
  externalAllowed?: boolean | undefined;
}) {
  const t = useT();
  const guilds = useGuilds();
  const mine = useMyExpressions(true);
  const [list, setList] = useState<GuildSticker[] | null>(null);
  const [packs, setPacks] = useState<StickerPack[]>([]);
  const [error, setError] = useState(false);
  const [q, setQ] = useState('');
  useEffect(() => {
    if (!guildId) return;
    let live = true;
    loadGuildStickers(guildId)
      .then((l) => live && setList(l))
      .catch(() => live && setError(true));
    void loadDefaultPacks(guildId).then((p) => live && setPacks(p));
    return () => {
      live = false;
    };
  }, [guildId]);
  const shown = useMemo(() => pickableStickers(list ?? [], q), [list, q]);
  const external = useMemo(() => {
    const order = new Map(guilds.map((g, i) => [g.id, i]));
    return (mine ?? [])
      .filter((s) => s.guild.id !== guildId && s.usable_externally)
      .map((s) => ({
        id: s.guild.id,
        name: s.guild.name,
        stickers: pickableStickers(s.stickers, q),
      }))
      .filter((s) => s.stickers.length > 0)
      .sort((a, b) => (order.get(a.id) ?? 1e9) - (order.get(b.id) ?? 1e9));
  }, [mine, guilds, guildId, q]);
  const shownDefaults = useMemo(
    () =>
      pickableStickers(
        packs.flatMap((p) => p.stickers),
        q,
      ),
    [packs, q],
  );
  if (!guildId && external.length === 0)
    return <p className="lng-muted lng-expr-note">{t('lounge.expressionPicker.serversOnly')}</p>;
  if (guildId && error)
    return <p className="lng-muted lng-expr-note">{t('lounge.expressionPicker.loadFailed')}</p>;
  if (guildId && !list)
    return <p className="lng-muted lng-expr-note">{t('lounge.expressionPicker.loading')}</p>;
  const own = list ?? [];
  const greyedTip = t('lounge.expressionPicker.externalNotAllowed');
  return (
    <div className="lng-emoji">
      <input
        type="search"
        className="lng-emoji-search"
        placeholder={t('lounge.expressionPicker.stickerPlaceholder')}
        aria-label={t('lounge.expressionPicker.searchStickers')}
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      <div className="lng-emoji-scroll">
        {shownDefaults.length > 0 && guildId && (
          <section aria-label={t('lounge.expressionPicker.defaultStickers')}>
            <h3 className="lng-sticker-section">{t('lounge.expressionPicker.defaultStickers')}</h3>
            <div className="lng-sticker-grid">
              {shownDefaults.map((s) => (
                <StickerTile key={s.id} guildId={guildId} sticker={s} onPick={onPick} />
              ))}
            </div>
          </section>
        )}
        {shownDefaults.length > 0 && shown.length > 0 && (
          <h3 className="lng-sticker-section">{t('lounge.expressionPicker.serverStickers')}</h3>
        )}
        {shown.length === 0 && shownDefaults.length === 0 && external.length === 0 ? (
          <p className="lng-muted">
            {own.length === 0 && packs.length === 0
              ? t('lounge.expressionPicker.noStickersYet')
              : t('lounge.expressionPicker.noStickersFound')}
          </p>
        ) : (
          shown.length > 0 &&
          guildId && (
            <div className="lng-sticker-grid">
              {shown.map((s) => (
                <StickerTile key={s.id} guildId={guildId} sticker={s} onPick={onPick} />
              ))}
            </div>
          )
        )}
        {external.map((s) => (
          <section
            key={s.id}
            aria-label={s.name}
            data-external={s.id}
            data-greyed={!externalAllowed || undefined}
          >
            <h3 className="lng-sticker-section" title={externalAllowed ? undefined : greyedTip}>
              {s.name}
            </h3>
            <div className="lng-sticker-grid" title={externalAllowed ? undefined : greyedTip}>
              {s.stickers.map((st) => (
                <StickerTile
                  key={st.id}
                  guildId={s.id}
                  sticker={st}
                  onPick={onPick}
                  greyed={!externalAllowed}
                />
              ))}
            </div>
          </section>
        ))}
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
  onGif,
  onCustomEmoji,
  externalEmoji = true,
  externalStickers = true,
}: {
  initial?: ExpressionTab;
  guildId: string | undefined;
  onEmoji: (emoji: string) => void;
  onSticker: (s: GuildSticker) => void;
  onGif?: (g: Gif) => void;
  onCustomEmoji?: (e: GuildEmoji) => void;
  /** The channel grants USE_EXTERNAL_EMOJIS / USE_EXTERNAL_STICKERS (D-376); true in a DM. */
  externalEmoji?: boolean;
  externalStickers?: boolean;
}) {
  const t = useT();
  const [tab, setTab] = useState<ExpressionTab>(initial);
  const tabs: [ExpressionTab, string][] = [
    ['gifs', t('lounge.expressionPicker.tabGifs')],
    ['stickers', t('lounge.expressionPicker.tabStickers')],
    ['emoji', t('lounge.expressionPicker.tabEmoji')],
  ];
  return (
    <div className="lng-expr">
      <div
        role="tablist"
        aria-label={t('lounge.expressionPicker.expressions')}
        className="lng-expr-tabs"
      >
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
        {tab === 'gifs' &&
          (onGif ? (
            <GifPicker onPick={onGif} />
          ) : (
            <p className="lng-muted lng-expr-note">{t('lounge.expressionPicker.gifsNotSetUp')}</p>
          ))}
        {tab === 'stickers' && (
          <StickerPicker guildId={guildId} onPick={onSticker} externalAllowed={externalStickers} />
        )}
        {tab === 'emoji' && (
          <EmojiPicker
            onPick={onEmoji}
            guildId={guildId}
            onPickCustom={onCustomEmoji}
            externalAllowed={externalEmoji}
          />
        )}
      </div>
    </div>
  );
}
