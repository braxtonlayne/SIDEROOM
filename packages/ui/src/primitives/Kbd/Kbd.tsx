import type { HTMLAttributes } from "react";
import { cx } from "../../util/cx";
import styles from "./Kbd.module.css";

/** DS-PRIM-013: a single keyboard key cap. */
export function Kbd({ className, ...rest }: HTMLAttributes<HTMLElement>) {
  return <kbd {...rest} className={cx(styles.kbd, className)} />;
}

export interface KbdComboProps extends HTMLAttributes<HTMLSpanElement> {
  /** e.g. ["Ctrl", "K"]. */
  keys: string[];
}

/** DS-PRIM-013: a key combination, rendered as `Ctrl + K` with a readable accessible name. */
export function KbdCombo({ keys, className, ...rest }: KbdComboProps) {
  return (
    <span role="img" {...rest} className={cx(styles.combo, className)} aria-label={keys.join(" plus ")}>
      {keys.map((k, i) => (
        <span key={`${k}-${i}`} aria-hidden="true" className={styles.part}>
          {i > 0 && <span className={styles.plus}>+</span>}
          <Kbd>{k}</Kbd>
        </span>
      ))}
    </span>
  );
}
