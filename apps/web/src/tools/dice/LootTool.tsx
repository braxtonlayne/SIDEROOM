import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useGuilds } from '../../lounge/data';
import { useViewer } from '../../settings/server/hooks';
import { Field, Problem, Root } from '../dev/parts';
import type { ToolProps } from '../registry';
import { pickWeighted } from './engine';
import {
  type LootScope,
  type LootTable,
  createLoot,
  deleteLoot,
  listLoot,
  lootRowsToText,
  parseLootRows,
  updateLoot,
} from './lootApi';

const MAX_HISTORY = 10;

/** Loot tables: weighted rows saved to the account or to a server, rolled in the browser. */
export default function LootTool(_props: ToolProps) {
  const t = useT();
  const guilds = useGuilds();
  const [scope, setScope] = useState<LootScope>(null);
  return (
    <Root label={t('tools.loot.name')}>
      <Field label={t('tools.loot.scope')}>
        <select value={scope ?? ''} onChange={(e) => setScope(e.target.value === '' ? null : e.target.value)}>
          <option value="">{t('tools.loot.mine')}</option>
          {guilds.map((g) => (
            <option key={g.id} value={g.id}>
              {g.name}
            </option>
          ))}
        </select>
      </Field>
      {scope === null ? <LootBody key="mine" scope={null} canWrite /> : <GuildLoot key={scope} guildId={scope} />}
    </Root>
  );
}

function GuildLoot({ guildId }: { guildId: string }) {
  const viewer = useViewer(guildId);
  return <LootBody scope={guildId} canWrite={viewer.ready && viewer.can('MANAGE_GUILD')} />;
}

function LootBody({ scope, canWrite }: { scope: LootScope; canWrite: boolean }) {
  const t = useT();
  const [tables, setTables] = useState<LootTable[] | null>(null);
  const [selected, setSelected] = useState<string>('');
  const [name, setName] = useState('');
  const [rowsText, setRowsText] = useState('');
  const [problem, setProblem] = useState<string | null>(null);
  const [drops, setDrops] = useState<string[]>([]);

  const load = useCallback(async () => {
    try {
      setTables(await listLoot(scope));
      setProblem(null);
    } catch {
      setTables([]);
      setProblem(t('tools.loot.loadFailed'));
    }
  }, [scope, t]);
  useEffect(() => {
    void load();
  }, [load]);

  const current = tables?.find((x) => x.id === selected) ?? null;
  const pick = (id: string) => {
    setSelected(id);
    const tbl = tables?.find((x) => x.id === id);
    setName(tbl?.name ?? '');
    setRowsText(tbl ? lootRowsToText(tbl.rows) : '');
    setDrops([]);
  };

  const errorText = (e: unknown) =>
    e instanceof ApiError && e.status === 409
      ? t('tools.loot.conflict')
      : e instanceof ApiError && e.status === 403
        ? t('tools.loot.denied')
        : t('tools.loot.saveFailed');

  const save = async (asNew: boolean) => {
    const rows = parseLootRows(rowsText);
    const input = { name: name.trim(), rows };
    if (!input.name || rows.length === 0 || rows.every((r) => r.weight === 0)) {
      setProblem(t('tools.loot.invalid'));
      return;
    }
    try {
      const saved = current && !asNew ? await updateLoot(scope, current.id, input) : await createLoot(scope, input);
      await load();
      setSelected(saved.id);
      setProblem(null);
    } catch (e) {
      setProblem(errorText(e));
    }
  };

  const remove = async () => {
    if (!current) return;
    try {
      await deleteLoot(scope, current.id);
      setSelected('');
      setName('');
      setRowsText('');
      await load();
    } catch (e) {
      setProblem(errorText(e));
    }
  };

  const roll = async () => {
    // What is in the box is what rolls, saved or not.
    const rows = parseLootRows(rowsText);
    const i = await pickWeighted(rows.map((r) => r.weight));
    if (i >= 0 && rows[i]) setDrops((d) => [rows[i]!.label, ...d].slice(0, MAX_HISTORY));
  };

  return (
    <>
      {tables === null ? (
        <p className="ud-muted" role="status">
          {t('tools.loot.loading')}
        </p>
      ) : (
        <Field label={t('tools.loot.table')}>
          <select value={selected} onChange={(e) => pick(e.target.value)}>
            <option value="">{t('tools.loot.new')}</option>
            {tables.map((x) => (
              <option key={x.id} value={x.id}>
                {x.name}
              </option>
            ))}
          </select>
        </Field>
      )}
      {!canWrite ? <p className="ud-muted">{t('tools.loot.readOnly')}</p> : null}
      <Field label={t('tools.loot.nameLabel')}>
        <input value={name} maxLength={80} disabled={!canWrite} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label={t('tools.loot.rows')}>
        <textarea
          className="ud-mono"
          value={rowsText}
          placeholder={t('tools.loot.rowsPlaceholder')}
          onChange={(e) => setRowsText(e.target.value)}
        />
      </Field>
      <div className="ud-row">
        <button type="button" className="ud-btn ud-btn-primary" onClick={() => void roll()}>
          {t('tools.loot.roll')}
        </button>
        <button type="button" className="ud-btn" disabled={!canWrite} onClick={() => void save(false)}>
          {current ? t('tools.loot.save') : t('tools.loot.create')}
        </button>
        {current ? (
          <>
            <button type="button" className="ud-btn" disabled={!canWrite} onClick={() => void save(true)}>
              {t('tools.loot.saveCopy')}
            </button>
            <button type="button" className="ud-btn" disabled={!canWrite} onClick={() => void remove()}>
              {t('tools.loot.delete')}
            </button>
          </>
        ) : null}
      </div>
      {problem ? <Problem>{problem}</Problem> : null}
      <div role="status" aria-live="polite">
        {drops[0] ? <p className="ud-winner">{t('tools.loot.dropped', { label: drops[0] })}</p> : null}
      </div>
      {drops.length > 1 ? (
        <ol className="ud-history" aria-label={t('tools.loot.history')}>
          {drops.slice(1).map((d, i) => (
            <li key={i}>{d}</li>
          ))}
        </ol>
      ) : null}
    </>
  );
}
