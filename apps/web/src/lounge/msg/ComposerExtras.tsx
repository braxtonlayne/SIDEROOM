import { Markdown } from '@sideroom/markdown-react';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../../i18n';
import {
  loadGuildStickers,
  pickableStickers,
  StickerTile,
  type GuildSticker,
} from '../components/ExpressionPicker';
import { parseMarkdown, useMarkdownReady } from '../markdown';

/** The box holds one short word (or two) and nothing else: only then are stickers offered. */
export function stickerQuery(text: string): string | null {
  const q = text.trim();
  if (q.length < 2 || q.length > 32 || q.includes('\n')) return null;
  if (/^[/@#:]/.test(q) || /[<>]/.test(q)) return null;
  return q;
}

/**
 * Sticker suggestions above the message box (LNG-USET-051): stickers of this server whose name starts with or
 * contains what was typed. A click sends the sticker, like the picker.
 */
export function StickerSuggestions({
  guildId,
  text,
  onPick,
}: {
  guildId: string | undefined;
  text: string;
  onPick: (s: GuildSticker) => void;
}) {
  const t = useT();
  const q = stickerQuery(text);
  const wanted = q !== null;
  const [list, setList] = useState<GuildSticker[]>([]);
  useEffect(() => {
    if (!guildId || !wanted) return;
    let live = true;
    void loadGuildStickers(guildId)
      .then((l) => live && setList(l))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [guildId, wanted]);
  if (!guildId || !q) return null;
  const lower = q.toLowerCase();
  const matches = pickableStickers(list, q)
    .filter((s) => s.name.toLowerCase().includes(lower))
    .sort(
      (a, b) =>
        Number(b.name.toLowerCase().startsWith(lower)) -
        Number(a.name.toLowerCase().startsWith(lower)),
    )
    .slice(0, 6);
  if (matches.length === 0) return null;
  return (
    <div
      className="lng-sticker-suggest"
      role="group"
      aria-label={t('lounge.composer.stickerSuggestions')}
    >
      {matches.map((s) => (
        <StickerTile key={s.id} guildId={guildId} sticker={s} onPick={onPick} />
      ))}
    </div>
  );
}

const HAS_MARKDOWN = /[*_~`|>]|^\s{0,3}(#{1,3}|-|\d+\.)\s|\[.+\]\(.+\)/m;

/** Shows the rendered Markdown under the message box (LNG-USET-051) once the text uses any formatting. */
export function MarkdownPreview({ text }: { text: string }) {
  const t = useT();
  useMarkdownReady();
  const show = HAS_MARKDOWN.test(text);
  const nodes = useMemo(() => (show ? parseMarkdown(text) : []), [show, text]);
  if (!show) return null;
  return (
    <div className="lng-md-preview" role="region" aria-label={t('lounge.composer.markdownPreview')}>
      <Markdown nodes={nodes} />
    </div>
  );
}
