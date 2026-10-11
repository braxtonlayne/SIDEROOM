import type { HTMLAttributes } from "react";
import { cx } from "../../util/cx";
import { Avatar } from "./Avatar";
import styles from "./AvatarStack.module.css";

export interface AvatarStackItem {
  id?: string;
  name: string;
  src?: string | null;
}

export interface AvatarStackProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  items: AvatarStackItem[];
  /** Max avatars shown before the "+N" overflow chip. Default 4. */
  max?: number;
  size?: 24 | 32 | 40;
  /** Accessible group name, e.g. "Members in this room". */
  label?: string;
}

/** DS-PRIM-009: overlapping avatars with a +N overflow chip. */
export function AvatarStack({ items, max = 4, size = 32, label, className, ...rest }: AvatarStackProps) {
  const shown = items.slice(0, max);
  const extra = items.length - shown.length;
  const names = items.map((i) => i.name).join(", ");
  return (
    <div
      role="group"
      aria-label={label ?? (names || "No members")}
      {...rest}
      className={cx(styles.stack, className)}
      style={{ "--stack-size": `${size}px` } as React.CSSProperties}
    >
      {shown.map((it, i) => (
        <span key={it.id ?? `${it.name}-${i}`} className={styles.item} title={it.name}>
          <Avatar name={it.name} src={it.src} size={size} />
        </span>
      ))}
      {extra > 0 && (
        <span className={cx(styles.item, styles.more)} role="img" aria-label={`and ${extra} more`}>
          <span aria-hidden="true">+{extra}</span>
        </span>
      )}
    </div>
  );
}
