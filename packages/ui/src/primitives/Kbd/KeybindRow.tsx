import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "../../util/cx";
import { KbdCombo } from "./Kbd";
import styles from "./Kbd.module.css";

export interface KeybindRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** What the shortcut does. */
  action: ReactNode;
  /** Keys, e.g. ["Ctrl", "K"]. */
  keys: string[];
}

/** A read-only "action ... keys" row; place inside a `<dl>` (it renders a `div` wrapping `dt`/`dd`). */
export function KeybindRow({ action, keys, className, ...rest }: KeybindRowProps) {
  return (
    <div {...rest} className={cx(styles.row, className)}>
      <dt className={styles.action}>{action}</dt>
      <dd className={styles.keys}>
        <KbdCombo keys={keys} />
      </dd>
    </div>
  );
}
