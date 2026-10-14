import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { EMOJI_CATEGORIES } from './emoji';

/**
 * Missing from @sideroom/ui: a compact emoji picker (search + categories).
 * Server sections, skin tones, favourites and custom emoji are follow-ups (see open questions).
 */
export function EmojiPicker({ onPick }: { onPick: (emoji: string) => void }) {
  const t = useT();
  const [q, setQ] = useState('');
  const groups = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return EMOJI_CATEGORIES.map((c) => ({
      name: c.name,
      emoji: needle ? c.emoji.filter(([n]) => n.includes(needle)) : c.emoji,
    })).filter((c) => c.emoji.length);
  }, [q]);
  return (
    <div className="lng-emoji">
      <input
        type="search"
        className="lng-emoji-search"
        placeholder={t('lounge.emojiPicker.placeholder')}
        aria-label={t('lounge.emojiPicker.search')}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        data-autofocus
      />
      <div className="lng-emoji-scroll">
        {groups.length === 0 && <p className="lng-muted">{t('lounge.emojiPicker.none')}</p>}
        {groups.map((g) => (
          <section key={g.name} aria-label={g.name}>
            <h3 className="lng-emoji-h">{g.name}</h3>
            <div className="lng-emoji-grid">
              {g.emoji.map(([name, ch]) => (
                <button
                  key={name}
                  type="button"
                  className="lng-emoji-btn"
                  aria-label={name.replace(/_/g, ' ')}
                  onClick={() => onPick(ch)}
                >
                  {ch}
                </button>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
