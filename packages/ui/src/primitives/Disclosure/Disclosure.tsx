import { useId, useState, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./Disclosure.module.css";

export interface CollapsibleProps {
  /** Trigger content (e.g. a channel category name). */
  title: ReactNode;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** "category" is the compact uppercase Discord-style category header. */
  variant?: "default" | "category";
  headingLevel?: 2 | 3 | 4 | 5 | 6;
  className?: string;
  children?: ReactNode;
}

/** DS-PRIM-008: disclosure button + region; also the collapsible channel category. */
export function Collapsible({
  title,
  open,
  defaultOpen = false,
  onOpenChange,
  variant = "default",
  headingLevel = 3,
  className,
  children,
}: CollapsibleProps) {
  const id = useId();
  const [internal, setInternal] = useState(defaultOpen);
  const isOpen = open ?? internal;
  const H = `h${headingLevel}` as "h3";
  const toggle = () => {
    if (open === undefined) setInternal(!isOpen);
    onOpenChange?.(!isOpen);
  };
  return (
    <div className={cx(styles.item, styles[variant], className)} data-open={isOpen || undefined}>
      <H className={styles.heading}>
        <button
          type="button"
          className={styles.trigger}
          aria-expanded={isOpen}
          aria-controls={`${id}-panel`}
          id={`${id}-trigger`}
          onClick={toggle}
        >
          <span className={styles.chevron} aria-hidden="true" />
          <span>{title}</span>
        </button>
      </H>
      <div id={`${id}-panel`} role="region" aria-labelledby={`${id}-trigger`} hidden={!isOpen} className={styles.panel}>
        {children}
      </div>
    </div>
  );
}

export interface AccordionItem {
  id: string;
  title: ReactNode;
  content: ReactNode;
}

export interface AccordionProps {
  items: AccordionItem[];
  /** Allow several panels open at once (default false: single). */
  multiple?: boolean;
  defaultOpen?: string[];
  headingLevel?: 2 | 3 | 4 | 5 | 6;
  className?: string;
}

/** DS-PRIM-008: a stack of Collapsibles; single-open unless `multiple`. */
export function Accordion({ items, multiple = false, defaultOpen = [], headingLevel = 3, className }: AccordionProps) {
  const [openIds, setOpenIds] = useState<string[]>(defaultOpen);
  return (
    <div className={cx(styles.accordion, className)}>
      {items.map((it) => (
        <Collapsible
          key={it.id}
          title={it.title}
          headingLevel={headingLevel}
          open={openIds.includes(it.id)}
          onOpenChange={(o) =>
            setOpenIds((cur) => (o ? (multiple ? [...cur, it.id] : [it.id]) : cur.filter((x) => x !== it.id)))
          }
        >
          {it.content}
        </Collapsible>
      ))}
    </div>
  );
}
