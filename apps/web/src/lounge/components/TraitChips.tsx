// Server traits (J-07, LNG-SSET-007): up to five short labels with an emoji, shown as chips on the directory card,
// the per-server profile dialog and the settings preview.
import { useEffect, useState } from 'react';
import { fetchEmojiUrl } from '../msg/CustomEmoji';
import './traits.css';

export interface TraitView {
  emoji_id?: string | null;
  emoji_name?: string | null;
  label: string;
}

function TraitEmoji({ id, name }: { id?: string | null; name?: string | null }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!id) return;
    let live = true;
    void fetchEmojiUrl(id).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [id]);
  if (id) {
    return url ? (
      <img className="lng-trait-emoji" src={url} alt="" aria-hidden="true" />
    ) : name ? (
      <span aria-hidden="true">{`:${name}:`}</span>
    ) : null;
  }
  return name ? <span aria-hidden="true">{name}</span> : null;
}

/** The traits as a row of chips; renders nothing for an empty list. */
export function TraitChips({
  traits,
  label,
}: {
  traits: readonly TraitView[] | null | undefined;
  /** Accessible name of the list. */
  label: string;
}) {
  if (!traits?.length) return null;
  return (
    <ul className="lng-traits" aria-label={label}>
      {traits.map((tr, i) => (
        <li key={`${i}:${tr.label}`} className="lng-trait">
          <TraitEmoji id={tr.emoji_id} name={tr.emoji_name} />
          <span>{tr.label}</span>
        </li>
      ))}
    </ul>
  );
}
