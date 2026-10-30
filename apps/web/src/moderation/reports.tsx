// Report entry points (XC-MOD-002, MOD-RPT-010). Reporting is behind the `moderation.reports` flag (off by
// default, D-216), so none of this renders until the operator turns it on. Choosing Report opens `ReportDialog`,
// which sends the ids and the note to POST /moderation/reports; the server snapshots the evidence.
import { Button, type MenuEntry } from '@sideroom/ui';
import { useCallback, useEffect } from 'react';
import { create } from 'zustand';
import { apiFetch } from '../api/client';
import { useT } from '../i18n';
import { onDispatch } from '../store/events';

/** The feature id registered in sr-features (`ModerationReportsFeature`). */
export const REPORTS_FLAG = 'moderation.reports';

export type ReportKind =
  'message' | 'user' | 'channel' | 'guild' | 'library' | 'cinema' | 'status_ring';

export interface ReportTarget {
  kind: ReportKind;
  /** Id of the thing being reported (snowflake). */
  id: string;
  /** A message report names the channel the message is in. */
  channelId?: string;
  /** A user report may name the server it was filed from, so that server's moderators get it. */
  guildId?: string;
  /** The person behind a message or user report, for "also block". */
  authorId?: string;
}

interface ReportsState {
  /** Resolved `moderation.reports` flag. Default off. */
  enabled: boolean;
  /** The target whose Report entry was chosen; non-null while the dialog is open. */
  target: ReportTarget | null;
}

export const useReports = create<ReportsState>(() => ({
  // Off until the server says otherwise (`GET /sr/features/flags`, D-216).
  enabled: false,
  target: null,
}));

/** The client-visible platform flags (D-216), as the server last sent them. A flag that is absent is off. */
export const useClientFlags = create<{ flags: Record<string, boolean> }>(() => ({ flags: {} }));

/** Replace the whole flag map (boot read, or `SR_FEATURE_UPDATE`, G-81) and derive what depends on it. */
export function setClientFlags(flags: Record<string, boolean>): void {
  useClientFlags.setState({ flags });
  setReportsEnabled(flags[REPORTS_FLAG] === true);
}

/** Whether a client-visible platform flag is on. */
export function useClientFlag(id: string): boolean {
  return useClientFlags((s) => s.flags[id] === true);
}

/** Reads the client-visible platform flags once (D-216). A failure leaves the flags as they were. */
export async function loadClientFlags(): Promise<void> {
  try {
    const res = await apiFetch<{ flags: Record<string, boolean> }>('/sr/features/flags', {
      retries: 0,
    });
    setClientFlags(res.flags ?? {});
  } catch {
    // Signed out, or the server predates the endpoint: reporting stays hidden.
  }
}

/** The map inside a gateway `SR_FEATURE_UPDATE` (`{scope: "platform", flags}`), or null for anything else. */
export function flagsOfUpdate(event: string, data: unknown): Record<string, boolean> | null {
  if (event !== 'SR_FEATURE_UPDATE') return null;
  const d = data as { scope?: unknown; flags?: unknown } | null;
  if (d?.scope !== 'platform' || !d.flags || typeof d.flags !== 'object') return null;
  const out: Record<string, boolean> = {};
  for (const [k, v] of Object.entries(d.flags as Record<string, unknown>)) out[k] = v === true;
  return out;
}

/**
 * Mounted once at the app root: loads the flags at boot and replaces the map whenever the gateway says a
 * client-visible platform flag changed (`SR_FEATURE_UPDATE`, G-81). Renders nothing.
 */
export function ReportsFlagSync() {
  useEffect(() => {
    void loadClientFlags();
    return onDispatch((event, data) => {
      const flags = flagsOfUpdate(event, data);
      if (flags) setClientFlags(flags);
    });
  }, []);
  return null;
}

/** Set the resolved flag (called by the flag feed). */
export function setReportsEnabled(enabled: boolean): void {
  useReports.setState({ enabled });
}

export function openReport(target: ReportTarget): void {
  useReports.setState({ target });
}

export function closeReport(): void {
  useReports.setState({ target: null });
}

export function useReportsEnabled(): boolean {
  return useReports((s) => s.enabled);
}

/** Menu entry factory: returns `null` when the flag is off, so callers can spread it away. */
export function useReportEntry(): (target: ReportTarget) => MenuEntry | null {
  const t = useT();
  const enabled = useReportsEnabled();
  return useCallback(
    (target: ReportTarget): MenuEntry | null => {
      if (!enabled) return null;
      const label =
        target.kind === 'message'
          ? t('moderation.report.menuItemMessage')
          : target.kind === 'user'
            ? t('moderation.report.menuItemUser')
            : target.kind === 'channel'
              ? t('moderation.report.menuItemChannel')
              : target.kind === 'guild'
                ? t('moderation.report.menuItemGuild')
                : t('moderation.report.menuItem');
      return { label, danger: true, onSelect: () => openReport(target) };
    },
    [t, enabled],
  );
}

/** A standalone Report button for pages without a menu (library series, cinema title). */
export function ReportButton({ target }: { target: ReportTarget }) {
  const t = useT();
  const enabled = useReportsEnabled();
  if (!enabled) return null;
  return (
    <Button variant="secondary" onClick={() => openReport(target)}>
      {t('moderation.report.button')}
    </Button>
  );
}

/** `[entry]` or `[]`, for spreading into a menu array. */
export function entryList(entry: MenuEntry | null): MenuEntry[] {
  return entry ? [entry] : [];
}
