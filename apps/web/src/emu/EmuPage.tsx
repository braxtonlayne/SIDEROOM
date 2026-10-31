import { Button, Notice } from '@sideroom/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { formatNumber, useT } from '../i18n';
import type { Mount } from '../settings/user/storage/types';
import { useCurrentUser, useGuilds } from '../store/hooks';
import { BiosPanel } from './BiosPanel';
import { CoopBar } from './coop/CoopBar';
import { blobScriptsAllowed, reloadForEmulatorPolicy } from './cspProbe';
import { isInVoice, useVoiceSession } from '../voice/session';
import { CORES, PLATFORM_LABEL, type Platform } from './cores';
import './emu.css';
import { loadMapping, saveMapping, type PadMapping } from './gamepad';
import { MappingDialog } from './MappingDialog';
import { listOwnMounts, listRoms, listRomsInBucket, romPlatform, type RomItem } from './romApi';
import { STATE_SLOTS } from './saves';
import { useEmuGame, type GameBios, type GameError } from './useEmuGame';

const sizeText = (n: number) =>
  n < 1024 ** 2 ? `${Math.max(1, Math.round(n / 1024))} KiB` : `${(n / 1024 ** 2).toFixed(1)} MiB`;

/** Own-bucket ROMs the catalog has not indexed yet are added to the catalog's list (by mount and key). */
function merge(indexed: RomItem[], fresh: RomItem[]): RomItem[] {
  const seen = new Set(indexed.map((r) => `${r.mount_id}:${r.key}`));
  return [...indexed, ...fresh.filter((r) => !seen.has(`${r.mount_id}:${r.key}`))];
}

function ErrorNotice({ error }: { error: GameError }) {
  const t = useT();
  switch (error.kind) {
    case 'core-missing':
      return (
        <Notice variant="warning" title={t('games.error.coreMissingTitle')}>
          {t('games.error.coreMissing', {
            core: typeof error.core === 'string' ? error.core : error.core.label,
          })}
        </Notice>
      );
    case 'webgl2':
      return <Notice variant="warning" title={t('games.error.webgl2')} />;
    case 'platform':
      return <Notice variant="warning" title={t('games.error.platform')} />;
    case 'bios':
      return (
        <Notice variant="warning" title={t('games.error.biosTitle')}>
          {t('games.error.bios')}
        </Notice>
      );
    case 'rom':
      return <Notice variant="danger" title={t('games.error.rom')} />;
    default:
      return (
        <Notice variant="danger" title={t('games.error.failed')}>
          {error.detail}
        </Notice>
      );
  }
}

/**
 * The emulator page (ACT-EMU-001), route `/games`. Lists the ROMs of the member's own bucket and of the servers they
 * play on (`view_games`), runs one in a canvas, and keeps saves on the member's own mount. Loaded only after sign-in.
 */
export function EmuPage() {
  const t = useT();
  const me = useCurrentUser();
  const guilds = useGuilds();
  const [roms, setRoms] = useState<RomItem[] | null>(null);
  const [mounts, setMounts] = useState<Mount[]>([]);
  const [error, setError] = useState(false);
  const [selected, setSelected] = useState<RomItem | null>(null);
  const [filter, setFilter] = useState('');
  const [mapping, setMapping] = useState<PadMapping>(() =>
    loadMapping(typeof localStorage === 'undefined' ? null : localStorage),
  );
  const [mapOpen, setMapOpen] = useState(false);
  const [bios, setBios] = useState<GameBios | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  // 'reload': reached in-app during a call, the page must load again for the emulator policy (cspProbe.ts).
  const [policy, setPolicy] = useState<'ok' | 'reload' | 'blocked'>('ok');

  useEffect(() => {
    // Dev servers send the policy report-only; the probe matters for a production document.
    if (!import.meta.env.PROD) return;
    let live = true;
    void blobScriptsAllowed().then((ok) => {
      if (!live || ok) return;
      const inCall = isInVoice(useVoiceSession.getState());
      if (!inCall && reloadForEmulatorPolicy()) return;
      setPolicy(inCall ? 'reload' : 'blocked');
    });
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const [indexed, own] = await Promise.all([listRoms(), listOwnMounts().catch(() => [])]);
        if (!live) return;
        setMounts(own);
        let all = indexed;
        for (const m of own) {
          if (m.status !== 'active' || !m.cors_ok_at) continue;
          try {
            all = merge(
              all,
              (await listRomsInBucket(m.id)).map((r) => ({ ...r, mount_label: m.label })),
            );
          } catch {
            /* a bucket that cannot be listed adds nothing */
          }
        }
        if (live) setRoms(all.sort((a, b) => a.title.localeCompare(b.title)));
      } catch {
        if (live) setError(true);
      }
    })();
    return () => {
      live = false;
    };
  }, []);

  const game = useEmuGame({ rom: selected, canvasRef, mounts, bios, mapping });
  const guildName = useCallback(
    (id?: string | null) => guilds.find((g) => g.id === id)?.name ?? '',
    [guilds],
  );

  const shown = useMemo(
    () => (roms ?? []).filter((r) => r.title.toLowerCase().includes(filter.trim().toLowerCase())),
    [roms, filter],
  );
  const hasPs1 = (roms ?? []).some((r) => romPlatform(r) === 'ps1') || game.needsBios;
  const playing = game.status.phase !== 'idle' && selected !== null;

  return (
    <main className="emu-page">
      <header className="emu-head">
        <h1>{t('games.page.title')}</h1>
        <p>{t('games.page.intro')}</p>
        <p className="emu-dim">{t('games.page.legal')}</p>
      </header>

      {error && <Notice variant="danger" title={t('games.page.loadFailed')} />}
      {policy === 'reload' && (
        <Notice variant="warning" title={t('games.policy.reloadTitle')} data-testid="emu-policy">
          {t('games.policy.reload')}
          <div>
            <Button size="sm" variant="secondary" onClick={() => window.location.assign('/games')}>
              {t('games.policy.reloadButton')}
            </Button>
          </div>
        </Notice>
      )}
      {policy === 'blocked' && (
        <Notice variant="danger" title={t('games.policy.blockedTitle')} data-testid="emu-policy">
          {t('games.policy.blocked')}
        </Notice>
      )}

      <section
        className="emu-stage"
        hidden={!playing}
        aria-label={t('games.player.label')}
        ref={stageRef}
      >
        <div className="emu-bar">
          <strong>{selected?.title}</strong>
          <span className="emu-dim">
            {selected && romPlatform(selected)
              ? PLATFORM_LABEL[romPlatform(selected) as Platform]
              : ''}
          </span>
          <span className="emu-spacer" />
          {game.status.phase === 'running' && (
            <Button variant="secondary" size="sm" onClick={game.pause}>
              {t('games.player.pause')}
            </Button>
          )}
          {game.status.phase === 'paused' && (
            <Button variant="secondary" size="sm" onClick={game.resume}>
              {t('games.player.resume')}
            </Button>
          )}
          <Button variant="secondary" size="sm" onClick={game.restart} disabled={!game.session}>
            {t('games.player.restart')}
          </Button>
          <Button variant="secondary" size="sm" onClick={() => setMapOpen(true)}>
            {t('games.player.controls')}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void stageRef.current?.requestFullscreen?.()}
          >
            {t('games.player.fullscreen')}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setSelected(null)}>
            {t('games.player.close')}
          </Button>
        </div>
        {/* In a server voice call: host this game for the others (ACT-EMU-002). */}
        {game.session && <CoopBar />}
        <canvas
          key={selected?.id}
          ref={canvasRef}
          className="emu-canvas"
          data-testid="emu-canvas"
          data-phase={game.status.phase}
          width={256}
          height={240}
          tabIndex={0}
          aria-label={t('games.player.canvas')}
        />
        {game.status.phase === 'loading' && (
          <p role="status" className="emu-dim">
            {t(`games.loading.${game.status.stage}`, {
              percent: formatNumber(Math.round(game.status.fraction * 100)),
            })}
          </p>
        )}
        {game.status.phase === 'error' && <ErrorNotice error={game.status.error} />}
        <div className="emu-saves">
          {STATE_SLOTS.map((s) => (
            <span key={s} className="emu-slot">
              <Button
                variant="secondary"
                size="sm"
                disabled={!game.session || game.save.kind === 'none'}
                onClick={() => void game.saveState(s)}
              >
                {t('games.saves.save', { slot: s })}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                disabled={!game.session || game.save.kind === 'none'}
                onClick={() => void game.loadState(s)}
              >
                {t('games.saves.load', { slot: s })}
              </Button>
            </span>
          ))}
        </div>
        <p className="emu-dim" role="status" data-testid="emu-save-status">
          {game.save.kind === 'none' && t('games.saves.noMount')}
          {game.save.kind === 'ready' && t('games.saves.ready', { mount: game.save.mountLabel })}
          {game.save.kind === 'saving' && t('games.saves.saving')}
          {game.save.kind === 'saved' && t('games.saves.saved')}
          {game.save.kind === 'error' && t('games.saves.failed')}
        </p>
      </section>

      {hasPs1 && me && <BiosPanel mounts={mounts} userId={me.id} bios={bios} onBios={setBios} />}

      <section aria-labelledby="emu-list-h">
        <h2 id="emu-list-h">{t('games.list.title')}</h2>
        <label className="emu-filter">
          <span>{t('games.list.filter')}</span>
          <input type="search" value={filter} onChange={(e) => setFilter(e.target.value)} />
        </label>
        {roms === null && !error && <p role="status">{t('games.list.loading')}</p>}
        {roms !== null && roms.length === 0 && (
          <Notice variant="info" title={t('games.list.emptyTitle')}>
            {t('games.list.empty')}
          </Notice>
        )}
        <ul className="emu-list">
          {shown.map((r) => {
            const p = romPlatform(r);
            const core = r.core && r.core in CORES ? CORES[r.core as keyof typeof CORES] : null;
            return (
              <li key={r.id} className="emu-rom">
                <div>
                  <strong>{r.title}</strong>
                  <div className="emu-dim">
                    {p ? PLATFORM_LABEL[p] : t('games.list.platformUnknown')}
                    {core ? ` - ${core.label}` : ''} - {sizeText(r.size)} -{' '}
                    {r.scope === 'guild'
                      ? t('games.list.fromServer', {
                          server: guildName(r.guild_id) || r.mount_label,
                        })
                      : t('games.list.fromYou')}
                  </div>
                </div>
                <Button onClick={() => setSelected(r)}>{t('games.list.play')}</Button>
              </li>
            );
          })}
        </ul>
      </section>

      <footer className="emu-foot emu-dim">
        {t('games.page.cores')}{' '}
        {Object.values(CORES)
          .map((c) => `${c.label} (${c.licence})`)
          .join(', ')}
        .
      </footer>

      <MappingDialog
        open={mapOpen}
        mapping={mapping}
        onClose={() => setMapOpen(false)}
        onSave={(m) => {
          setMapping(m);
          saveMapping(typeof localStorage === 'undefined' ? null : localStorage, m);
        }}
      />
    </main>
  );
}
