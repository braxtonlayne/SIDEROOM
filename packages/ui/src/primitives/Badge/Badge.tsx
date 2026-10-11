import type { HTMLAttributes } from "react";
import { cx } from "../../util/cx";
import styles from "./Badge.module.css";

export type BadgeVariant = "neutral" | "accent" | "success" | "warning" | "danger";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
  /** Pill shape (fully rounded). */
  pill?: boolean;
}

export function Badge({ variant = "neutral", pill, className, ...rest }: BadgeProps) {
  return <span {...rest} className={cx(styles.badge, styles[variant], pill && styles.pill, className)} />;
}

export interface CounterBadgeProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  count: number;
  /** Counts above this render as `max+`. Default 99. */
  max?: number;
  /** Noun for the accessible name, e.g. "unread messages". */
  noun?: string;
  /** Hide when count is zero (default true). */
  hideZero?: boolean;
}

export function CounterBadge({ count, max = 99, noun = "notifications", hideZero = true, className, ...rest }: CounterBadgeProps) {
  if (count <= 0 && hideZero) return null;
  const text = count > max ? `${max}+` : String(Math.max(0, count));
  return (
    <span {...rest} aria-label={`${count} ${noun}`} className={cx(styles.counter, className)}>
      <span aria-hidden="true">{text}</span>
    </span>
  );
}
