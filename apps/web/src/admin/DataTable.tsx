// Shared admin table (ADM-UI-002): server-side (cursor) pagination, a filter slot, a column chooser that
// remembers its choice, and CSV export of what is loaded and visible. Pages pass a `Paged` from usePaged.
import { Button, Checkbox, EmptyState, Notice } from '@sideroom/ui';
import { useId, useMemo, useState, type ReactNode } from 'react';
import { useT } from '../i18n';
import { LoadMore, type Paged } from './ui';

export interface Column<T> {
  /** Stable id (also the storage key for the chooser). */
  id: string;
  header: string;
  cell: (row: T) => ReactNode;
  /** Plain value for CSV export; columns without one are skipped by the export. */
  csv?: (row: T) => string | number | boolean | null | undefined;
  /** Hidden until the staff member turns it on in the chooser. */
  defaultHidden?: boolean;
  /** Always shown (the chooser lists it as disabled), e.g. the row's name. */
  locked?: boolean;
}

/** One CSV field: quoted when needed, and formula-looking text is defused so a sheet never evaluates it. */
export function csvField(v: string | number | boolean | null | undefined): string {
  let s = v === null || v === undefined ? '' : String(v);
  if (/^[=+\-@\t\r]/.test(s) && typeof v === 'string') s = `'${s}`;
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export function buildCsv<T>(rows: readonly T[], columns: readonly Column<T>[]): string {
  const cols = columns.filter((c) => c.csv);
  const lines = [cols.map((c) => csvField(c.header)).join(',')];
  for (const r of rows) lines.push(cols.map((c) => csvField(c.csv!(r))).join(','));
  return lines.join('\r\n') + '\r\n';
}

function readHidden(key: string | undefined, columns: readonly Column<never>[]): string[] {
  const dflt = columns.filter((c) => c.defaultHidden).map((c) => c.id);
  if (!key) return dflt;
  try {
    const raw = localStorage.getItem(`sr.admin.cols.${key}`);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    if (Array.isArray(parsed) && parsed.every((x) => typeof x === 'string')) return parsed;
  } catch {
    /* storage unavailable: use defaults */
  }
  return dflt;
}

function saveHidden(key: string | undefined, hidden: string[]) {
  if (!key) return;
  try {
    localStorage.setItem(`sr.admin.cols.${key}`, JSON.stringify(hidden));
  } catch {
    /* ignore */
  }
}

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/csv;charset=utf-8' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export interface DataTableProps<T> {
  /** Accessible name of the table. */
  label: string;
  columns: Column<T>[];
  paged: Paged<T>;
  rowKey: (row: T) => string;
  /** Filter controls rendered in the toolbar, before the chooser and export buttons. */
  filters?: ReactNode;
  /** Per-row actions, rendered in a trailing column with a visually hidden header. */
  actions?: (row: T) => ReactNode;
  emptyTitle: string;
  emptyDescription?: string;
  /** Persists the column chooser (omit for none). Also names the CSV file. */
  tableId: string;
}

export function DataTable<T>({
  label,
  columns,
  paged,
  rowKey,
  filters,
  actions,
  emptyTitle,
  emptyDescription,
  tableId,
}: DataTableProps<T>) {
  const t = useT();
  const panelId = useId();
  const [hidden, setHidden] = useState(() => readHidden(tableId, columns as Column<never>[]));
  const [choosing, setChoosing] = useState(false);
  const shown = useMemo(
    () => columns.filter((c) => c.locked || !hidden.includes(c.id)),
    [columns, hidden],
  );
  const toggle = (id: string, on: boolean) => {
    const next = on ? hidden.filter((h) => h !== id) : [...hidden, id];
    setHidden(next);
    saveHidden(tableId, next);
  };
  const exportCsv = () => download(`${tableId}.csv`, buildCsv(paged.items, shown));
  const canExport = shown.some((c) => c.csv) && paged.items.length > 0;

  return (
    <>
      <div className="ss-toolbar adm-dt-bar">
        {filters}
        <div className="adm-actions adm-dt-tools">
          <Button
            variant="secondary"
            aria-expanded={choosing}
            aria-controls={panelId}
            onClick={() => setChoosing((v) => !v)}
          >
            {t('admin.dataTable.columns')}
          </Button>
          <Button variant="secondary" disabled={!canExport} onClick={exportCsv}>
            {t('admin.dataTable.exportCsv')}
          </Button>
        </div>
      </div>
      {choosing && (
        <fieldset id={panelId} className="adm-dt-chooser">
          <legend className="ss-sub">{t('admin.dataTable.chooseColumns')}</legend>
          {columns.map((c) => (
            <Checkbox
              key={c.id}
              label={c.header}
              checked={c.locked || !hidden.includes(c.id)}
              disabled={c.locked}
              onChange={(e) => toggle(c.id, e.target.checked)}
            />
          ))}
        </fieldset>
      )}
      {paged.error && (
        <Notice variant="danger" className="ss-status" role="alert">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={emptyTitle}
          {...(emptyDescription ? { description: emptyDescription } : {})}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={label}>
            <thead>
              <tr>
                {shown.map((c) => (
                  <th key={c.id} scope="col">
                    {c.header}
                  </th>
                ))}
                {actions && (
                  <th scope="col">
                    <span className="adm-vh">{t('admin.dataTable.actions')}</span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {paged.items.map((row) => (
                <tr key={rowKey(row)}>
                  {shown.map((c) => (
                    <td key={c.id}>{c.cell(row)}</td>
                  ))}
                  {actions && <td>{actions(row)}</td>}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="ss-sub adm-dt-count" role="status">
        {t('admin.dataTable.loaded', { count: paged.items.length })}
      </p>
      <LoadMore paged={paged} />
    </>
  );
}
