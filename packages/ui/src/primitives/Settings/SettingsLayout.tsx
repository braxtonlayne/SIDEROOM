import { useEffect, useRef, useState, type ReactNode } from "react";
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
  className?: string;
  children: ReactNode;
}

/** DS-PRIM-015: full-screen settings (sidebar sections, ESC close, unsaved-changes bar). */
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
  className,
  children,
}: SettingsLayoutProps) {
  const [nudge, setNudge] = useState(0);
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

  return (
    <div role="dialog" aria-modal={inline ? undefined : true} aria-label={title} className={cx(styles.root, inline && styles.inline, className)}>
      <nav className={styles.sidebar} aria-label={title}>
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
                    onClick={() => onSelect(s.id)}
                  >
                    {s.label}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
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
