import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { cx } from "../../util/cx";
import styles from "./Settings.module.css";

export interface SettingsSection {
  id: string;
  label: ReactNode;
  /** Sections with the same group render under one small heading. */
  group?: string;
  danger?: boolean;
}

export interface SettingsLayoutProps {
  /** Dialog name, e.g. "User settings". */
  title: string;
  sections: SettingsSection[];
  activeId: string;
  onSelect: (id: string) => void;
  onClose: () => void;
  /** True when there are unsaved edits: shows the bar and makes Escape warn instead of closing. */
  dirty?: boolean;
  onSave?: () => void;
  onReset?: () => void;
  saving?: boolean;
  /** Render in flow instead of covering the viewport (stories, tests, embedding). */
  inline?: boolean;
  /** While dirty, section clicks shake the unsaved-changes bar instead of navigating (LNG-SSET-002). */
  blockNavWhenDirty?: boolean;
  /** Let the content column use the full width (tables, matrices). */
  wide?: boolean;
  /** Label of the button that expands the section list on narrow screens. */
  navToggleLabel?: string;
  className?: string;
  /** Pinned under the section list (log out, version). */
  sidebarFooter?: ReactNode;
  children: ReactNode;
}

/**
 * DS-PRIM-015: full-screen settings (sidebar sections, ESC close, unsaved-changes bar).
 * The section list is one tab stop (roving tabindex; Up/Down/Home/End move) and collapses behind a toggle on
 * narrow screens.
 */
export function SettingsLayout({
  title,
  sections,
  activeId,
  onSelect,
  onClose,
  dirty,
  onSave,
  onReset,
  saving,
  inline,
  blockNavWhenDirty,
  wide,
  navToggleLabel = "Sections",
  className,
  sidebarFooter,
  children,
}: SettingsLayoutProps) {
  const [nudge, setNudge] = useState(0);
  const [navOpen, setNavOpen] = useState(false);
  const [focusId, setFocusId] = useState<string | null>(null);
  const navRefs = useRef(new Map<string, HTMLButtonElement>());
  const navId = useId();
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (dirtyRef.current) setNudge((n) => n + 1);
      else onCloseRef.current();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const groups: Array<{ name: string | undefined; items: SettingsSection[] }> = [];
  for (const s of sections) {
    const last = groups[groups.length - 1];
    if (last && last.name === s.group) last.items.push(s);
    else groups.push({ name: s.group, items: [s] });
  }
  const active = sections.find((s) => s.id === activeId);
  const tabStop = sections.some((s) => s.id === focusId) ? focusId : active ? active.id : sections[0]?.id;

  const onNavKey = (e: ReactKeyboardEvent<HTMLElement>) => {
    const ids = sections.map((s) => s.id);
    const cur = ids.findIndex((id) => navRefs.current.get(id) === document.activeElement);
    if (cur < 0) return;
    let next: number;
    switch (e.key) {
      case "ArrowDown":
        next = (cur + 1) % ids.length;
        break;
      case "ArrowUp":
        next = (cur - 1 + ids.length) % ids.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = ids.length - 1;
        break;
      default:
        return;
    }
    e.preventDefault();
    const id = ids[next];
    if (id === undefined) return;
    setFocusId(id);
    navRefs.current.get(id)?.focus();
  };

  const choose = (id: string) => {
    setFocusId(id);
    setNavOpen(false);
    if (dirty && blockNavWhenDirty && id !== activeId) setNudge((n) => n + 1);
    else onSelect(id);
  };

  return (
    <div
      role="dialog"
      aria-modal={inline ? undefined : true}
      aria-label={title}
      className={cx(styles.root, inline && styles.inline, wide && styles.wide, className)}
    >
      <div className={styles.sidebar} data-open={navOpen}>
        <button
          type="button"
          className={styles.navToggle}
          aria-expanded={navOpen}
          aria-controls={`${navId}-nav`}
          onClick={() => setNavOpen((o) => !o)}
        >
          {navToggleLabel}
          {typeof active?.label === "string" ? `: ${active.label}` : ""}
        </button>
        {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- arrow keys bubble from the section buttons (roving tabindex) */}
        <nav id={`${navId}-nav`} className={styles.nav} aria-label={title} onKeyDown={onNavKey}>
          {groups.map((g, gi) => (
            <div key={`${g.name ?? "g"}-${gi}`} className={styles.group}>
              {g.name && <p className={styles.groupName}>{g.name}</p>}
              <ul className={styles.list}>
                {g.items.map((s) => (
                  <li key={s.id}>
                    <button
                      type="button"
                      className={cx(styles.navItem, s.danger && styles.danger)}
                      aria-current={s.id === activeId ? "page" : undefined}
                      tabIndex={s.id === tabStop ? 0 : -1}
                      ref={(el) => {
                        if (el) navRefs.current.set(s.id, el);
                        else navRefs.current.delete(s.id);
                      }}
                      onClick={() => choose(s.id)}
                    >
                      {s.label}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ))}
          {sidebarFooter && <div className={styles.footer}>{sidebarFooter}</div>}
        </nav>
      </div>
      <div className={styles.main}>
        <div className={styles.topline}>
          <button type="button" className={styles.close} onClick={() => (dirty ? setNudge((n) => n + 1) : onClose())}>
            <span aria-hidden="true" className={styles.closeX}>
              ×
            </span>
            <span className={styles.closeText}>
              Close <span className={styles.esc}>ESC</span>
            </span>
            <span className={styles.srOnly}>settings</span>
          </button>
        </div>
        <main className={styles.content} aria-label={typeof active?.label === "string" ? active.label : title}>
          {children}
        </main>
        {dirty && (
          <div key={nudge} role="alert" className={styles.bar} data-nudge={nudge > 0 || undefined}>
            <span>Careful, you have unsaved changes!</span>
            <span className={styles.barActions}>
              <button type="button" className={styles.reset} onClick={onReset}>
                Reset
              </button>
              <button type="button" className={styles.save} onClick={onSave} disabled={saving}>
                {saving ? "Saving..." : "Save changes"}
              </button>
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
