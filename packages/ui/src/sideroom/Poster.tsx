import { useRef, type HTMLAttributes, type ReactNode } from "react";
import { cx } from "../util/cx";
import { ProgressBar } from "../primitives/Progress/ProgressBar";
import { ArrowLink } from "./Editorial";
import styles from "./Poster.module.css";

export interface PosterCardProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  title: string;
  /** Second line, e.g. "Manga · Ch. 86". */
  subtitle?: ReactNode;
  /** Third muted line, e.g. genres. */
  meta?: ReactNode;
  src?: string | null;
  href?: string;
  /** Corner tag, e.g. "#1 TRENDING". */
  tag?: ReactNode;
  /** 0..100 reading/watching progress; omit to hide. */
  progress?: number;
  /** Rating shown at the right of the subtitle row. */
  rating?: ReactNode;
  /** Hover/focus actions (e.g. the "+" add-to-list button). Should be real buttons with labels. */
  actions?: ReactNode;
}

/** DS-SR-005: 2:3 poster with title block, optional corner tag, progress and hover/focus actions. */
export function PosterCard({ title, subtitle, meta, src, href, tag, progress, rating, actions, className, ...rest }: PosterCardProps) {
  const art = (
    <span className={styles.art}>
      {src ? <img src={src} alt="" loading="lazy" draggable={false} /> : <span className={styles.fallback}>{title.slice(0, 1)}</span>}
      {tag && <span className={styles.tag}>{tag}</span>}
      {progress !== undefined && (
        <ProgressBar className={styles.progress} value={progress} label={`${title} progress`} size={3} />
      )}
    </span>
  );
  return (
    <article {...rest} className={cx(styles.card, className)}>
      <div className={styles.frame}>
        {href ? (
          <a href={href} className={styles.link} aria-label={title} data-poster-link="">
            {art}
          </a>
        ) : (
          art
        )}
        {actions && <div className={styles.actions}>{actions}</div>}
      </div>
      <h3 className={styles.title}>{title}</h3>
      {(subtitle || rating) && (
        <p className={styles.sub}>
          <span>{subtitle}</span>
          {rating && <span className={styles.rating}>{rating}</span>}
        </p>
      )}
      {meta && <p className={styles.meta}>{meta}</p>}
    </article>
  );
}

export interface PosterRowProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  title: ReactNode;
  /** "Explore all ↗" style link in the heading row. */
  exploreHref?: string;
  exploreLabel?: ReactNode;
  children: ReactNode;
}

/** DS-SR-005: titled horizontal scroller of posters with prev/next arrows and an "Explore all" link. */
export function PosterRow({ title, exploreHref, exploreLabel = "Explore all", className, children, ...rest }: PosterRowProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const page = (dir: 1 | -1) => {
    const el = scroller.current;
    el?.scrollBy?.({ left: dir * (el?.clientWidth ?? 0) * 0.9, behavior: "smooth" });
  };
  return (
    <section {...rest} className={cx(styles.row, className)}>
      <div className={styles.rowHead}>
        <h2 className={styles.rowTitle}>{title}</h2>
        <div className={styles.rowTools}>
          {exploreHref && <ArrowLink href={exploreHref}>{exploreLabel}</ArrowLink>}
          <button type="button" className={styles.arrowBtn} aria-label="Scroll back" onClick={() => page(-1)}>
            <span aria-hidden="true">←</span>
          </button>
          <button type="button" className={styles.arrowBtn} aria-label="Scroll forward" onClick={() => page(1)}>
            <span aria-hidden="true">→</span>
          </button>
        </div>
      </div>
      <div ref={scroller} className={styles.scroller}>
        {children}
      </div>
    </section>
  );
}
