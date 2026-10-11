import type { HTMLAttributes, ReactNode, SVGAttributes } from "react";
import { cx } from "../util/cx";
import styles from "./Chrome.module.css";

export interface DoorLogoProps extends Omit<SVGAttributes<SVGSVGElement>, "children"> {
  size?: number;
  /** Accessible name. Omit when the mark sits beside the wordmark (decorative). */
  label?: string;
}

/**
 * DS-SR-008: the `[|]` door mark: square brackets framing an ajar door. Uses `currentColor`
 * so it follows the theme. Static favicon variants live in packages/ui/assets/.
 */
export function DoorLogo({ size = 28, label, className, ...rest }: DoorLogoProps) {
  return (
    <svg
      {...rest}
      className={cx(styles.logo, className)}
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="square"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      <path d="M10 4H5v24h5M22 4h5v24h-5" />
      <path d="M16 8v16" strokeWidth="3" />
    </svg>
  );
}

export interface SrHeaderProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
  /** Where the logo/wordmark links to. */
  homeHref?: string;
  tagline?: ReactNode;
  /** Right-hand action, e.g. an ArrowButton "Member access". */
  action?: ReactNode;
}

/** DS-SR-002: banner with door logo, wordmark, tagline and a right-aligned action. */
export function SrHeader({ homeHref = "/", tagline, action, className, ...rest }: SrHeaderProps) {
  return (
    <header {...rest} className={cx(styles.header, className)}>
      <a href={homeHref} className={styles.brand}>
        <DoorLogo />
        <span className={styles.wordmark}>SIDE ROOM</span>
      </a>
      {tagline && <p className={styles.tagline}>{tagline}</p>}
      {action && <div className={styles.action}>{action}</div>}
    </header>
  );
}

export interface SrFooterLink {
  label: ReactNode;
  href: string;
}

export interface SrFooterProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
  /** Lead of the sign-off; the default is the brand line. */
  lead?: ReactNode;
  accent?: ReactNode;
  links?: SrFooterLink[];
}

/** DS-SR-002: footer sign-off ("Not everywhere. *Just here.*") with optional links. */
export function SrFooter({ lead = "Not everywhere.", accent = "Just here.", links, className, ...rest }: SrFooterProps) {
  return (
    <footer {...rest} className={cx(styles.footer, className)}>
      <p className={styles.signoff}>
        {lead} <em className={styles.accent}>{accent}</em>
      </p>
      {links && links.length > 0 && (
        <nav aria-label="Footer" className={styles.footerLinks}>
          {links.map((l) => (
            <a key={l.href} href={l.href}>
              {l.label}
            </a>
          ))}
        </nav>
      )}
    </footer>
  );
}

export interface SectionTopBarItem {
  id: string;
  label: ReactNode;
  href: string;
}

export interface SectionTopBarProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
  items: SectionTopBarItem[];
  activeId?: string;
  homeHref?: string;
  /** Slot at the right edge for the "in a voice room" indicator. */
  voiceSlot?: ReactNode;
}

/** DS-SR-003: `SIDE ROOM / The lounge  The cinema  The library` bar with an active underline. */
export function SectionTopBar({ items, activeId, homeHref = "/", voiceSlot, className, ...rest }: SectionTopBarProps) {
  return (
    <div {...rest} className={cx(styles.topBar, className)}>
      <a href={homeHref} className={styles.topBrand}>
        SIDE ROOM
      </a>
      <span className={styles.slash} aria-hidden="true">
        /
      </span>
      <nav aria-label="Sections" className={styles.topNav}>
        {items.map((it) => (
          <a key={it.id} href={it.href} className={styles.topLink} aria-current={it.id === activeId ? "page" : undefined}>
            {it.label}
          </a>
        ))}
      </nav>
      {voiceSlot && <div className={styles.voice}>{voiceSlot}</div>}
    </div>
  );
}
