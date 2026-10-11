import { useEffect, useId, useState } from "react";
import { cx } from "../../util/cx";
import { isHexColor } from "../../util/contrast";
import styles from "./ColorPicker.module.css";

export const DEFAULT_SWATCHES = [
  "#c2b08f",
  "#8c9a6a",
  "#5865f2",
  "#3ba55d",
  "#faa61a",
  "#ed4245",
  "#eb459e",
  "#9b59b6",
  "#1abc9c",
  "#e67e22",
  "#95a5a6",
  "#ece6d6",
];

interface EyeDropperLike {
  open: () => Promise<{ sRGBHex: string }>;
}

export interface ColorPickerProps {
  label: string;
  /** Current color as `#rrggbb`. */
  value?: string;
  defaultValue?: string;
  onValueChange?: (hex: string) => void;
  swatches?: string[];
  className?: string;
}

function normalize(hex: string): string {
  const h = hex.trim().toLowerCase();
  const withHash = h.startsWith("#") ? h : `#${h}`;
  if (/^#[0-9a-f]{3}$/.test(withHash)) {
    return `#${withHash[1]}${withHash[1]}${withHash[2]}${withHash[2]}${withHash[3]}${withHash[3]}`;
  }
  return withHash;
}

/** DS-PRIM-013: swatches + hex field + eyedropper (where the browser supports `EyeDropper`). */
export function ColorPicker({ label, value, defaultValue = "#c2b08f", onValueChange, swatches = DEFAULT_SWATCHES, className }: ColorPickerProps) {
  const id = useId();
  const [internal, setInternal] = useState(defaultValue);
  const current = value ?? internal;
  const [text, setText] = useState(current);
  useEffect(() => setText(current), [current]);
  const invalid = text !== current && !isHexColor(normalize(text));
  const canEyedrop = typeof window !== "undefined" && "EyeDropper" in window;

  const commit = (hex: string) => {
    const n = normalize(hex);
    if (!isHexColor(n)) return;
    if (value === undefined) setInternal(n);
    onValueChange?.(n);
  };

  return (
    <div role="group" aria-label={label} className={cx(styles.picker, className)}>
      <div className={styles.swatches}>
        {swatches.map((s) => (
          <button
            key={s}
            type="button"
            className={styles.swatch}
            style={{ background: s }}
            aria-label={s}
            aria-pressed={normalize(s) === current.toLowerCase()}
            onClick={() => commit(s)}
          />
        ))}
      </div>
      <div className={styles.row}>
        <span className={styles.preview} style={{ background: isHexColor(current) ? current : "transparent" }} aria-hidden="true" />
        <label htmlFor={`${id}-hex`} className={styles.hexLabel}>
          Hex
        </label>
        <input
          id={`${id}-hex`}
          className={styles.hex}
          value={text}
          maxLength={7}
          spellCheck={false}
          aria-invalid={invalid || undefined}
          onChange={(e) => {
            setText(e.target.value);
            if (isHexColor(normalize(e.target.value))) commit(e.target.value);
          }}
          onBlur={() => setText(current)}
        />
        {canEyedrop && (
          <button
            type="button"
            className={styles.dropper}
            onClick={async () => {
              try {
                const ctor = (window as unknown as { EyeDropper: new () => EyeDropperLike }).EyeDropper;
                const res = await new ctor().open();
                commit(res.sRGBHex);
              } catch {
                /* user cancelled */
              }
            }}
          >
            Pick from screen
          </button>
        )}
      </div>
    </div>
  );
}
