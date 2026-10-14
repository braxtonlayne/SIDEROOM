/** Share Target parameters (manifest `share_target`, XC-PWA-006). */
export interface SharedContent {
  title: string;
  text: string;
  url: string;
}

export function parseShare(search: string): SharedContent {
  const q = new URLSearchParams(search);
  return { title: q.get('title') ?? '', text: q.get('text') ?? '', url: q.get('url') ?? '' };
}

/** The message text to drop into the composer: text and link, without repeating a link already in the text. */
export function shareToDraft(s: SharedContent): string {
  const parts = [s.title && s.title !== s.text ? s.title : '', s.text];
  if (s.url && !s.text.includes(s.url)) parts.push(s.url);
  return parts
    .map((p) => p.trim())
    .filter(Boolean)
    .join('\n');
}
