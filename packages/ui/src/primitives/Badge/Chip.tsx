import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./Chip.module.css";

export interface PillProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "neutral" | "accent" | "success" | "warning" | "danger";
}

/** DS-PRIM-010: fully rounded, non-interactive status pill. */
export function Pill({ tone = "neutral", className, ...rest }: PillProps) {
  return <span {...rest} className={cx(styles.pill, styles[tone], className)} />;
}

export interface TagProps extends HTMLAttributes<HTMLSpanElement> {
  /** Optional leading glyph (e.g. server tag emblem); decorative. */
  icon?: ReactNode;
  /** Square-cornered outlined genre tag (default) or filled server-tag chip. */
  variant?: "outline" | "filled";
}

/** DS-PRIM-010: small label tag, used for genres and server tags. */
export function Tag({ icon, variant = "outline", className, children, ...rest }: TagProps) {
  return (
    <span {...rest} className={cx(styles.tag, styles[variant], className)}>
      {icon && (
        <span className={styles.icon} aria-hidden="true">
          {icon}
        </span>
      )}
      {children}
    </span>
  );
}

export interface ChipProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange"> {
  /** Toggle state (renders aria-pressed). Omit for a plain action chip. */
  selected?: boolean;
  icon?: ReactNode;
}

/** DS-PRIM-010: interactive chip (filter toggle or action). */
export function Chip({ selected, icon, type = "button", className, children, ...rest }: ChipProps) {
  return (
    <button
      {...rest}
      type={type}
      aria-pressed={selected}
      data-selected={selected || undefined}
      className={cx(styles.chip, className)}
    >
      {icon && (
        <span className={styles.icon} aria-hidden="true">
          {icon}
        </span>
      )}
      {children}
    </button>
  );
}
