import type { CSSProperties, HTMLAttributes } from "react";
import { cx } from "../../util/cx";
import styles from "./Progress.module.css";

export interface ProgressBarProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** Current value. Omit for an indeterminate bar. */
  value?: number;
  max?: number;
  /** Accessible name (required: progress bars are meaningless unlabelled). */
  label: string;
  variant?: "linear" | "circular";
  /** Linear thickness in px, or circular diameter in px. */
  size?: number;
}

/** DS-PRIM-011: determinate / indeterminate progress, linear or circular. */
export function ProgressBar({ value, max = 100, label, variant = "linear", size, className, style, ...rest }: ProgressBarProps) {
  const determinate = value !== undefined;
  const clamped = determinate ? Math.min(Math.max(value, 0), max) : 0;
  const pct = max > 0 ? (clamped / max) * 100 : 0;
  const aria = {
    role: "progressbar" as const,
    "aria-label": label,
    "aria-valuemin": 0,
    "aria-valuemax": max,
    "aria-valuenow": determinate ? clamped : undefined,
  };
  if (variant === "circular") {
    const d = size ?? 40;
    const stroke = Math.max(3, Math.round(d / 10));
    const r = (d - stroke) / 2;
    const c = 2 * Math.PI * r;
    return (
      <div
        {...rest}
        {...aria}
        className={cx(styles.circular, !determinate && styles.indeterminate, className)}
        style={{ width: d, height: d, ...style }}
      >
        <svg width={d} height={d} viewBox={`0 0 ${d} ${d}`} aria-hidden="true">
          <circle className={styles.circleTrack} cx={d / 2} cy={d / 2} r={r} strokeWidth={stroke} fill="none" />
          <circle
            className={styles.circleFill}
            cx={d / 2}
            cy={d / 2}
            r={r}
            strokeWidth={stroke}
            fill="none"
            strokeLinecap="round"
            strokeDasharray={c}
            strokeDashoffset={determinate ? c * (1 - pct / 100) : c * 0.75}
            transform={`rotate(-90 ${d / 2} ${d / 2})`}
          />
        </svg>
      </div>
    );
  }
  return (
    <div
      {...rest}
      {...aria}
      className={cx(styles.linear, !determinate && styles.indeterminate, className)}
      style={{ height: size ?? 6, ...style } as CSSProperties}
    >
      <div className={styles.bar} style={determinate ? { width: `${pct}%` } : undefined} />
    </div>
  );
}
