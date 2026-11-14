import { useState, type HTMLAttributes, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./Avatar.module.css";

export type AvatarStatus = "online" | "idle" | "dnd" | "offline";
const STATUS_LABEL: Record<AvatarStatus, string> = {
  online: "Online",
  idle: "Idle",
  dnd: "Do not disturb",
  offline: "Offline",
};

export interface AvatarProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  /** Display name; used for the alt text and the initials fallback. */
  name: string;
  src?: string | null;
  /** Q70: animated frame (GIF/WebP/APNG) shown on hover or focus; the static `src` is the default. */
  animatedSrc?: string | null;
  size?: 24 | 32 | 40 | 56 | 80;
  status?: AvatarStatus;
  /** Drawn in the status dot's place (same corner and size, a ring of the page colour); coloured as the status. */
  statusIcon?: ReactNode;
  /** Accessible name of the status when `statusIcon` replaces the dot; defaults to the status word. */
  statusLabel?: string;
  speaking?: boolean;
  /** Patron ring (D-256): a thin accent ring outside the avatar, drawn from tokens, no artwork. */
  ring?: boolean;
  /** Decoration slot: decorative overlay (frame, hat) above the image; rendered aria-hidden. */
  decoration?: ReactNode;
}

/** The first letter or digit of a word; brackets, quotes and other punctuation are skipped ("(owner)" gives "O"). */
const LETTER = /[\p{L}\p{N}]/u;
const leadLetter = (word: string) => Array.from(word).find((ch) => LETTER.test(ch));

export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  const letters = words.map(leadLetter).filter((c): c is string => !!c);
  // A name of symbols or emoji only keeps its first character.
  if (letters.length === 0) return Array.from(words[0]!)[0] ?? "?";
  const first = letters[0]!;
  const last = letters.length > 1 ? letters[letters.length - 1]! : "";
  return (first + last).toUpperCase();
}

export function Avatar({ name, src, animatedSrc, size = 40, status, statusIcon, statusLabel, speaking, ring, decoration, className, style, ...rest }: AvatarProps) {
  const [failed, setFailed] = useState<string | null>(null);
  const showImage = !!src && failed !== src;
  // An icon needs a little more room than a dot to read at all.
  const dotSize = statusIcon ? Math.max(12, size * 0.4) : Math.max(8, size * 0.28);
  const dot = { width: dotSize, height: dotSize };
  return (
    <span
      {...rest}
      className={cx(styles.avatar, speaking && styles.speaking, ring && styles.ring, className)}
      style={{ width: size, height: size, fontSize: Math.round(size * 0.4), ...style }}
      data-speaking={speaking || undefined}
      data-ring={ring || undefined}
    >
      {showImage ? (
        <>
          <img className={styles.image} src={src} alt={name} onError={() => setFailed(src)} draggable={false} />
          {animatedSrc && (
            <img className={styles.animated} src={animatedSrc} alt="" aria-hidden="true" draggable={false} />
          )}
        </>
      ) : (
        <span
          className={styles.fallback}
          {...(name ? { role: 'img', 'aria-label': name } : { 'aria-hidden': true })}
          data-initials={initials(name)}
        />
      )}
      {decoration && (
        <span className={styles.decoration} aria-hidden="true">
          {decoration}
        </span>
      )}
      {status && (
        <span
          className={cx(styles.status, styles[status], !!statusIcon && styles.statusIcon)}
          role="img"
          aria-label={statusLabel ?? STATUS_LABEL[status]}
          style={dot}
        >
          {statusIcon}
        </span>
      )}
    </span>
  );
}
