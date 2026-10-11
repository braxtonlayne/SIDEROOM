import type { HTMLAttributes } from "react";
import { cx } from "../../util/cx";
import styles from "./Loading.module.css";

export interface SpinnerProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  size?: "sm" | "md" | "lg";
  /** Accessible name announced to screen readers. */
  label?: string;
}

export function Spinner({ size = "md", label = "Loading", className, ...rest }: SpinnerProps) {
  return (
    <span role="status" aria-label={label} {...rest} className={cx(styles.spinner, styles[`spin-${size}`], className)}>
      <span className={styles.ring} aria-hidden="true" />
    </span>
  );
}

export interface SkeletonProps extends HTMLAttributes<HTMLDivElement> {
  width?: string | number;
  height?: string | number;
  circle?: boolean;
}

/** Decorative placeholder; hidden from assistive tech. Pair with a labelled busy region. */
export function Skeleton({ width, height = "1em", circle, className, style, ...rest }: SkeletonProps) {
  return (
    <div
      aria-hidden="true"
      {...rest}
      className={cx(styles.skeleton, circle && styles.circle, className)}
      style={{ width: circle ? height : width, height, ...style }}
    />
  );
}
