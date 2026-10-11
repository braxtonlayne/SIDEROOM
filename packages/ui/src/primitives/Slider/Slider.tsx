import { useId, useState, type CSSProperties, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./Slider.module.css";

export interface SliderMark {
  value: number;
  label?: ReactNode;
}

export interface SliderProps {
  /** Accessible name (visible label text). */
  label: ReactNode;
  min?: number;
  max?: number;
  step?: number;
  value?: number;
  defaultValue?: number;
  onValueChange?: (value: number) => void;
  marks?: SliderMark[];
  /** Show the current value next to the label. */
  showValue?: boolean;
  formatValue?: (value: number) => string;
  disabled?: boolean;
  className?: string;
  id?: string;
}

/** DS-PRIM-003: native range input (keyboard + SR for free) with optional tick marks. */
export function Slider({
  label,
  min = 0,
  max = 100,
  step = 1,
  value,
  defaultValue,
  onValueChange,
  marks,
  showValue,
  formatValue = String,
  disabled,
  className,
  id,
}: SliderProps) {
  const auto = useId();
  const inputId = id ?? auto;
  const [internal, setInternal] = useState(defaultValue ?? min);
  const current = value ?? internal;
  const span = max - min || 1;
  const pct = ((current - min) / span) * 100;
  return (
    <div className={cx(styles.slider, className)} data-disabled={disabled || undefined}>
      <div className={styles.head}>
        <label htmlFor={inputId} className={styles.label}>
          {label}
        </label>
        {showValue && (
          <output htmlFor={inputId} className={styles.value}>
            {formatValue(current)}
          </output>
        )}
      </div>
      <input
        id={inputId}
        type="range"
        className={styles.input}
        min={min}
        max={max}
        step={step}
        value={current}
        disabled={disabled}
        style={{ "--slider-pct": `${pct}%` } as CSSProperties}
        onChange={(e) => {
          const n = Number(e.target.value);
          if (value === undefined) setInternal(n);
          onValueChange?.(n);
        }}
      />
      {marks && marks.length > 0 && (
        <div className={styles.marks} aria-hidden="true">
          {marks.map((m) => (
            <span key={m.value} className={styles.mark} style={{ left: `${((m.value - min) / span) * 100}%` }}>
              <span className={styles.tick} />
              {m.label !== undefined && <span className={styles.markLabel}>{m.label}</span>}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export interface SegmentedOption {
  value: string;
  label: ReactNode;
  disabled?: boolean;
}

export interface SegmentedControlProps {
  /** Accessible name for the group. */
  label: string;
  options: SegmentedOption[];
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  name?: string;
  size?: "sm" | "md";
  disabled?: boolean;
  className?: string;
}

/** DS-PRIM-003: single-choice segmented control built on native radios (arrow keys work natively). */
export function SegmentedControl({
  label,
  options,
  value,
  defaultValue,
  onValueChange,
  name,
  size = "md",
  disabled,
  className,
}: SegmentedControlProps) {
  const auto = useId();
  const [internal, setInternal] = useState(defaultValue ?? options[0]?.value);
  const current = value ?? internal;
  return (
    <div role="radiogroup" aria-label={label} className={cx(styles.segmented, styles[`seg-${size}`], className)}>
      {options.map((o) => (
        <label
          key={o.value}
          className={styles.segment}
          data-checked={current === o.value || undefined}
          data-disabled={o.disabled || disabled || undefined}
        >
          <input
            type="radio"
            className={styles.segInput}
            name={name ?? auto}
            value={o.value}
            checked={current === o.value}
            disabled={o.disabled || disabled}
            onChange={() => {
              if (value === undefined) setInternal(o.value);
              onValueChange?.(o.value);
            }}
          />
          <span>{o.label}</span>
        </label>
      ))}
    </div>
  );
}
