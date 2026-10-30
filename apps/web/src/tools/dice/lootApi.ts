/**
 * UTL-DEV-001: the loot-table endpoints (`crates/sr-api/src/routes/tools/loot.rs`).
 * A member's own tables live on the account; a server's tables are written with MANAGE_GUILD.
 */
import { apiFetch } from '../../api/client';

export interface LootRow {
  label: string;
  /** 0 to 10000; 0 never drops. */
  weight: number;
}

export interface LootTable {
  id: string;
  scope: 'user' | 'guild';
  name: string;
  rows: LootRow[];
  updated_at: string;
}

export interface LootInput {
  name: string;
  rows: LootRow[];
}

/** `null` is the member's own account; a string is a server (guild) id. */
export type LootScope = string | null;

const base = (scope: LootScope) =>
  scope === null ? '/users/@me/tools/loot-tables' : `/guilds/${scope}/tools/loot-tables`;

export const listLoot = (scope: LootScope) => apiFetch<LootTable[]>(base(scope));

export const createLoot = (scope: LootScope, input: LootInput) =>
  apiFetch<LootTable>(base(scope), { method: 'POST', json: input, retries: 0 });

export const updateLoot = (scope: LootScope, id: string, input: LootInput) =>
  apiFetch<LootTable>(`${base(scope)}/${id}`, { method: 'PATCH', json: input, retries: 0 });

export const deleteLoot = (scope: LootScope, id: string) =>
  apiFetch<void>(`${base(scope)}/${id}`, { method: 'DELETE', retries: 0 });

/** Parse "Sword 3" / "Sword, 3" / "Sword" lines into rows (weight defaults to 1). */
export function parseLootRows(text: string): LootRow[] {
  const rows: LootRow[] = [];
  for (const line of text.split('\n')) {
    const s = line.trim();
    if (!s) continue;
    const m = /^(.*?)[\s,;:]+(\d{1,5})$/.exec(s);
    const label = (m ? m[1]! : s).trim();
    if (!label) continue;
    rows.push({ label, weight: m ? Math.min(10_000, Number(m[2])) : 1 });
  }
  return rows;
}

export const lootRowsToText = (rows: LootRow[]) =>
  rows.map((r) => `${r.label}, ${r.weight}`).join('\n');
