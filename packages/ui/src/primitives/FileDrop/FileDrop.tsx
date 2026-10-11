import { useId, useRef, useState, type DragEvent, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./FileDrop.module.css";

export interface FileRejection {
  file: File;
  reason: "type" | "size" | "count";
}

export interface FileDropProps {
  /** Accessible name / visible prompt, e.g. "Upload cover art". */
  label: ReactNode;
  hint?: ReactNode;
  /** `accept` attribute syntax: ".png,.jpg" or "image/*". */
  accept?: string;
  multiple?: boolean;
  /** Max bytes per file. */
  maxSize?: number;
  maxFiles?: number;
  disabled?: boolean;
  onFiles: (files: File[]) => void;
  onReject?: (rejections: FileRejection[]) => void;
  className?: string;
}

function matchesAccept(file: File, accept: string | undefined): boolean {
  if (!accept) return true;
  const rules = accept.split(",").map((r) => r.trim().toLowerCase()).filter(Boolean);
  const name = file.name.toLowerCase();
  const type = file.type.toLowerCase();
  return rules.some((r) => {
    if (r.startsWith(".")) return name.endsWith(r);
    if (r.endsWith("/*")) return type.startsWith(r.slice(0, -1));
    return type === r;
  });
}

/** Splits candidate files into accepted and rejected per type, size and count limits. */
export function filterFiles(
  files: File[],
  opts: Pick<FileDropProps, "accept" | "maxSize" | "maxFiles" | "multiple">,
): { accepted: File[]; rejected: FileRejection[] } {
  const accepted: File[] = [];
  const rejected: FileRejection[] = [];
  const cap = opts.multiple ? (opts.maxFiles ?? Infinity) : 1;
  for (const file of files) {
    if (!matchesAccept(file, opts.accept)) rejected.push({ file, reason: "type" });
    else if (opts.maxSize !== undefined && file.size > opts.maxSize) rejected.push({ file, reason: "size" });
    else if (accepted.length >= cap) rejected.push({ file, reason: "count" });
    else accepted.push(file);
  }
  return { accepted, rejected };
}

/** DS-PRIM-013: drag-and-drop zone that is also a keyboard-operable file picker button. */
export function FileDrop({ label, hint, accept, multiple, maxSize, maxFiles, disabled, onFiles, onReject, className }: FileDropProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const hintId = useId();
  const [over, setOver] = useState(false);

  const handle = (list: FileList | null) => {
    if (!list || list.length === 0) return;
    const { accepted, rejected } = filterFiles(Array.from(list), { accept, maxSize, maxFiles, multiple });
    if (accepted.length > 0) onFiles(accepted);
    if (rejected.length > 0) onReject?.(rejected);
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    if (!disabled) handle(e.dataTransfer.files);
  };

  return (
    <div
      className={cx(styles.drop, className)}
      data-over={over || undefined}
      data-disabled={disabled || undefined}
      onDragOver={(e) => {
        e.preventDefault();
        if (!disabled) setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
    >
      <button
        type="button"
        className={styles.button}
        disabled={disabled}
        aria-describedby={hint ? hintId : undefined}
        onClick={() => inputRef.current?.click()}
      >
        {label}
      </button>
      <span className={styles.or} aria-hidden="true">
        or drop {multiple ? "files" : "a file"} here
      </span>
      {hint && (
        <span id={hintId} className={styles.hint}>
          {hint}
        </span>
      )}
      <input
        ref={inputRef}
        type="file"
        className={styles.native}
        tabIndex={-1}
        aria-hidden="true"
        accept={accept}
        multiple={multiple}
        disabled={disabled}
        onChange={(e) => {
          handle(e.target.files);
          e.target.value = "";
        }}
      />
    </div>
  );
}
