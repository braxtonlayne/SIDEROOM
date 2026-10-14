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
  speaking?: boolean;
  /** Decoration slot: decorative overlay (frame, hat) above the image; rendered aria-hidden. */
  decoration?: ReactNode;
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  const first = Array.from(parts[0]!)[0] ?? "?";
  const last = parts.length > 1 ? (Array.from(parts[parts.length - 1]!)[0] ?? "") : "";
  return (first + last).toUpperCase();
}

export function Avatar({ name, src, animatedSrc, size = 40, status, speaking, decoration, className, style, ...rest }: AvatarProps) {
  const [failed, setFailed] = useState<string | null>(null);
  const showImage = !!src && failed !== src;
  return (
    <span
      {...rest}
      className={cx(styles.avatar, speaking && styles.speaking, className)}
      style={{ width: size, height: size, fontSize: Math.round(size * 0.4), ...style }}
      data-speaking={speaking || undefined}
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
        >
          {initials(name)}
        </span>
      )}
      {decoration && (
        <span className={styles.decoration} aria-hidden="true">
          {decoration}
        </span>
      )}
      {status && (
        <span
          className={cx(styles.status, styles[status])}
          role="img"
          aria-label={STATUS_LABEL[status]}
          style={{ width: Math.max(8, size * 0.28), height: Math.max(8, size * 0.28) }}
        />
      )}
    </span>
  );
}
