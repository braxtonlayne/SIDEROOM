import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type HTMLAttributes,
  type Key,
  type ReactNode,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { cx } from "../../util/cx";
import { observeRectWithFallback } from "../../util/virtual";
import styles from "./Scroll.module.css";

/* ------------------------------------------------------------ ScrollArea */

export interface ScrollAreaProps extends HTMLAttributes<HTMLDivElement> {
  /** `thin` shows a slim themed bar on hover/focus; `auto` is the platform default; `hidden` hides it. */
  scrollbar?: "thin" | "auto" | "hidden";
  orientation?: "vertical" | "horizontal" | "both";
}

/**
 * A scroll container with themed scrollbars. Keyboard users can scroll it: it is focusable
 * (tabIndex 0) and, when given an accessible name, exposed as a labelled region.
 */
export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(function ScrollArea(
  { scrollbar = "thin", orientation = "vertical", className, children, tabIndex = 0, ...rest },
  ref,
) {
  const named = !!(rest["aria-label"] || rest["aria-labelledby"]);
  return (
    <div
      role={named && !rest.role ? "region" : rest.role}
      {...rest}
      ref={ref}
      tabIndex={tabIndex}
      data-sr-part="scroll-area"
      className={cx(
        styles.scroll,
        orientation === "both" ? styles.both : orientation === "horizontal" ? styles.x : styles.y,
        styles[scrollbar],
        className,
      )}
    >
      {children}
    </div>
  );
});

/* ------------------------------------------------------------ VirtualList */

export interface VirtualListHandle {
  scrollToIndex: (index: number, align?: "start" | "center" | "end" | "auto") => void;
  element: HTMLDivElement | null;
}

export interface VirtualListProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  count: number;
  /** Estimated row height in px. Rows are measured after render, so variable heights work. */
  estimateSize: number;
  renderItem: (index: number) => ReactNode;
  getItemKey?: (index: number) => Key;
  overscan?: number;
  scrollbar?: ScrollAreaProps["scrollbar"];
  /** Rows to assume visible before layout exists (SSR, tests). Default 20. */
  initialVisible?: number;
  /** Shown instead of the list when `count` is 0. */
  empty?: ReactNode;
}

/** Vertical windowed list. Give it a bounded height (style/className); only visible rows are mounted. */
export const VirtualList = forwardRef<VirtualListHandle, VirtualListProps>(function VirtualList(
  {
    count,
    estimateSize,
    renderItem,
    getItemKey,
    overscan = 6,
    scrollbar,
    initialVisible = 20,
    empty,
    className,
    role = "list",
    ...rest
  },
  ref,
) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const v = useVirtualizer({
    count,
    getScrollElement: () => scroller.current,
    estimateSize: () => estimateSize,
    measureElement: (el, entry) =>
      Math.round(entry?.borderBoxSize?.[0]?.blockSize ?? el.getBoundingClientRect().height) || estimateSize,
    getItemKey,
    overscan,
    initialRect: { width: 0, height: estimateSize * initialVisible },
    observeElementRect: observeRectWithFallback({ width: 0, height: estimateSize * initialVisible }),
  });
  useImperativeHandle(ref, () => ({ scrollToIndex: (i, align) => v.scrollToIndex(i, { align }), element: scroller.current }), [v]);

  if (count === 0 && empty) return <>{empty}</>;
  return (
    <ScrollArea ref={scroller} scrollbar={scrollbar} className={className} role={role} {...rest}>
      <div className={styles.sizer} style={{ height: v.getTotalSize() }}>
        {v.getVirtualItems().map((item) => (
          <div
            key={item.key}
            ref={v.measureElement}
            data-index={item.index}
            role={role === "list" ? "listitem" : undefined}
            aria-setsize={role === "list" ? count : undefined}
            aria-posinset={role === "list" ? item.index + 1 : undefined}
            className={styles.row}
            style={{ transform: `translateY(${item.start}px)` }}
          >
            {renderItem(item.index)}
          </div>
        ))}
      </div>
    </ScrollArea>
  );
});

/* ------------------------------------------------------------ VirtualGrid */

export interface VirtualGridProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  count: number;
  renderItem: (index: number) => ReactNode;
  /** Fixed column count. Takes precedence over `minColumnWidth`. */
  columns?: number;
  /** Responsive: as many columns as fit with at least this width each. */
  minColumnWidth?: number;
  /** Height of each row in px (cells are uniform, e.g. poster cards). */
  rowHeight: number;
  gap?: number;
  overscan?: number;
  scrollbar?: ScrollAreaProps["scrollbar"];
  initialVisibleRows?: number;
  empty?: ReactNode;
}

/** Vertical windowed grid with uniform rows. Only rows in view (plus overscan) are mounted. */
export const VirtualGrid = forwardRef<HTMLDivElement, VirtualGridProps>(function VirtualGrid(
  {
    count,
    renderItem,
    columns,
    minColumnWidth = 160,
    rowHeight,
    gap = 12,
    overscan = 3,
    scrollbar,
    initialVisibleRows = 6,
    empty,
    className,
    role = "list",
    ...rest
  },
  ref,
) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(0);
  const setRef = useCallback(
    (el: HTMLDivElement | null) => {
      scroller.current = el;
      if (typeof ref === "function") ref(el);
      else if (ref) ref.current = el;
    },
    [ref],
  );

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const cols = Math.max(1, columns ?? (width > 0 ? Math.floor((width + gap) / (minColumnWidth + gap)) : 1));
  const rows = Math.ceil(count / cols);
  const pitch = rowHeight + gap;
  const v = useVirtualizer({
    count: rows,
    getScrollElement: () => scroller.current,
    estimateSize: () => pitch,
    overscan,
    initialRect: { width: 0, height: pitch * initialVisibleRows },
    observeElementRect: observeRectWithFallback({ width: 0, height: pitch * initialVisibleRows }),
  });

  if (count === 0 && empty) return <>{empty}</>;
  return (
    <ScrollArea ref={setRef} scrollbar={scrollbar} className={className} role={role} {...rest}>
      <div className={styles.sizer} style={{ height: Math.max(0, v.getTotalSize() - gap) }}>
        {v.getVirtualItems().map((row) => {
          const first = row.index * cols;
          const n = Math.min(cols, count - first);
          return (
            <div
              key={row.key}
              role="presentation"
              data-row={row.index}
              className={cx(styles.row, styles.gridRow)}
              style={{
                height: rowHeight,
                gap,
                gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
                transform: `translateY(${row.start}px)`,
              }}
            >
              {Array.from({ length: n }, (_, c) => {
                const index = first + c;
                return (
                  <div
                    key={index}
                    role={role === "list" ? "listitem" : undefined}
                    aria-setsize={role === "list" ? count : undefined}
                    aria-posinset={role === "list" ? index + 1 : undefined}
                    style={{ minWidth: 0, minHeight: 0 }}
                  >
                    {renderItem(index)}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </ScrollArea>
  );
});
