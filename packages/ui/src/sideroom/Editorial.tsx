import { forwardRef, type AnchorHTMLAttributes, type ButtonHTMLAttributes, type ElementType, type HTMLAttributes, type ReactNode } from "react";
import { cx } from "../util/cx";
import styles from "./Editorial.module.css";

/** DS-SR-001: hairline + letter-spaced gold caps ("— GOOD THINGS STAY BETWEEN US."). */
export function Eyebrow({ className, children, ...rest }: HTMLAttributes<HTMLParagraphElement>) {
  return (
    <p {...rest} className={cx(styles.eyebrow, className)}>
      <span className={styles.rule} aria-hidden="true" />
      <span>{children}</span>
    </p>
  );
}

export interface DisplayHeadingProps extends Omit<HTMLAttributes<HTMLHeadingElement>, "children"> {
  /** Heading level element. Default h1. */
  as?: "h1" | "h2" | "h3" | "h4";
  /** Plain lead text, e.g. "The internet, with the door". */
  children: ReactNode;
  /** Italic gold accent that follows, e.g. "closed." */
  accent?: ReactNode;
  size?: "md" | "lg" | "xl";
}

/** DS-SR-001: editorial serif heading with an italic accent span. */
export function DisplayHeading({ as = "h1", children, accent, size = "lg", className, ...rest }: DisplayHeadingProps) {
  const Tag = as as ElementType;
  return (
    <Tag {...rest} className={cx(styles.heading, styles[`size-${size}`], className)}>
      {children}
      {accent !== undefined && (
        <>
          {" "}
          <em className={styles.accent}>{accent}</em>
        </>
      )}
    </Tag>
  );
}

export interface ArrowLinkProps extends AnchorHTMLAttributes<HTMLAnchorElement> {
  /** Arrow glyph; the default points up-right. */
  arrow?: "↗" | "→" | "↓";
}

/** DS-SR-004: quiet text link with a trailing arrow. The arrow is decorative. */
export const ArrowLink = forwardRef<HTMLAnchorElement, ArrowLinkProps>(function ArrowLink(
  { arrow = "↗", className, children, ...rest },
  ref,
) {
  return (
    <a {...rest} ref={ref} className={cx(styles.arrowLink, className)}>
      <span>{children}</span>
      <span className={styles.arrow} aria-hidden="true">
        {arrow}
      </span>
    </a>
  );
});

interface SrButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  arrow?: boolean;
}

function makeButton(variantClass: string, displayName: string) {
  const C = forwardRef<HTMLButtonElement, SrButtonProps>(function SrButton(
    { arrow = false, type = "button", className, children, ...rest },
    ref,
  ) {
    return (
      <button {...rest} ref={ref} type={type} className={cx(styles.srButton, variantClass, className)}>
        <span>{children}</span>
        {arrow && (
          <span className={styles.arrow} aria-hidden="true">
            ↗
          </span>
        )}
      </button>
    );
  });
  C.displayName = displayName;
  return C;
}

/** DS-SR-004: primary near-square gold CTA ("Check invitation ↗"). */
export const GoldButton = makeButton(styles.gold!, "GoldButton");
/** DS-SR-004: outlined secondary CTA ("Edit lineup"). */
export const OutlineButton = makeButton(styles.outline!, "OutlineButton");
/** DS-SR-004: borderless button with a trailing arrow ("Member access ↗"). */
export const ArrowButton = makeButton(styles.plain!, "ArrowButton");
