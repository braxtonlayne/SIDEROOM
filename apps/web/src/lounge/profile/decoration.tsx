/**
 * Avatar decorations and profile effects (LNG-PRF-009). Both are free and come from approved packs, so the client
 * only ever names a file stem or an effect id the server sent; anything that does not look like one is ignored.
 * Decorations are images served from `/cdn/avatar-decorations/{asset}.png` (contract). Effects are built-in CSS
 * animations keyed by id; an unknown id draws nothing, and `prefers-reduced-motion` freezes them.
 */
import { useState, type ReactNode } from 'react';

const ASSET = /^[a-z0-9][a-z0-9_-]{0,63}$/i;

export const decorationUrl = (asset: string | null | undefined): string | null =>
  asset && ASSET.test(asset) ? `/cdn/avatar-decorations/${asset}.png` : null;

/** Effect ids the client knows how to draw. */
export const EFFECT_IDS = ['sparkle', 'aurora', 'snow', 'embers'] as const;
export type EffectId = (typeof EFFECT_IDS)[number];

/** The effect to draw: the server sends a slug, Discord an `{ id }` object; unknown ids draw nothing. */
export const effectOf = (e: string | { id: string } | null | undefined): EffectId | null => {
  const id = typeof e === 'string' ? e : e?.id;
  return id && (EFFECT_IDS as readonly string[]).includes(id) ? (id as EffectId) : null;
};

function Decoration({ src }: { src: string }) {
  const [failed, setFailed] = useState(false);
  if (failed) return null;
  return (
    <img className="prf-deco" src={src} alt="" draggable={false} onError={() => setFailed(true)} />
  );
}

/** The node for `<Avatar decoration={...}>`, or undefined when the person has no (valid) decoration. */
export function decorationNode(data: { asset: string } | null | undefined): ReactNode | undefined {
  const src = decorationUrl(data?.asset);
  return src ? <Decoration src={src} /> : undefined;
}

/** A decorative animated layer over the top of a profile surface. */
export function ProfileEffect({ effect }: { effect: EffectId | null }) {
  if (!effect) return null;
  return <div className="prf-effect" data-effect={effect} aria-hidden="true" />;
}
