// The jukebox of a voice channel (MUS-JB-002): what is playing, the queue, vote-skip, the decks and a picker
// for the member's own library. Sits above the call stage in a server voice channel.
import { Button, Icon } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { clock } from '../../watchparty/sync';
import { httpJukeboxApi } from './api';
import type { JukeboxController, JukeboxStatus } from './controller';
import { useJukebox } from './useJukebox';
import type { JukeboxApi, JukeboxItem, JukeboxState, LibraryTrack } from './types';
import './jukebox.css';

const STATUS_KEY = {
  off: 'lounge.jukebox.status.off',
  idle: 'lounge.jukebox.status.idle',
  loading: 'lounge.jukebox.status.loading',
  playing: 'lounge.jukebox.status.playing',
  paused: 'lounge.jukebox.status.paused',
  notShared: 'lounge.jukebox.status.notShared',
  streamed: 'lounge.jukebox.status.streamed',
  blocked: 'lounge.jukebox.status.blocked',
  error: 'lounge.jukebox.status.error',
} as const satisfies Record<JukeboxStatus, string>;

export function JukeboxStrip({
  channelId,
  meId,
  api = httpJukeboxApi,
}: {
  channelId: string | undefined;
  /** The signed-in member, for claiming the decks. */
  meId: string;
  api?: JukeboxApi;
}) {
  const t = useT();
  const { snapshot, controller, audioRef } = useJukebox(api);
  const [adding, setAdding] = useState(false);
  const [, setTick] = useState(0);
  const listening = snapshot.status !== 'off';

  // Re-render each second while listening so the position text follows the schedule.
  useEffect(() => {
    if (!listening) return;
    const h = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(h);
  }, [listening]);

  if (!channelId) return null;
  const s = snapshot.state;
  const expected = controller?.expectedMs() ?? s?.position_ms ?? 0;
  return (
    <aside
      className="jb-strip"
      aria-label={t('lounge.jukebox.label')}
      data-status={snapshot.status}
    >
      <audio ref={audioRef} className="jb-audio" data-testid="jukebox-audio" preload="auto" />
      <div className="jb-head">
        <Icon name="headphones" size={20} />
        <strong>{t('lounge.jukebox.title')}</strong>
        <span className="jb-muted" role="status" data-testid="jukebox-status">
          {t(STATUS_KEY[snapshot.status])}
        </span>
        {!listening ? (
          <Button
            size="sm"
            onClick={() => void controller?.start(channelId)}
            disabled={!controller}
          >
            {t('lounge.jukebox.listen')}
          </Button>
        ) : (
          <Button size="sm" variant="ghost" onClick={() => void controller?.leave()}>
            {t('lounge.jukebox.stop')}
          </Button>
        )}
      </div>
      {listening && s && controller ? (
        <Body
          s={s}
          status={snapshot.status}
          controller={controller}
          expectedMs={expected}
          adding={adding}
          setAdding={setAdding}
          api={api}
          meId={meId}
        />
      ) : null}
    </aside>
  );
}

function Body({
  s,
  status,
  controller,
  expectedMs,
  adding,
  setAdding,
  api,
  meId,
}: {
  s: JukeboxState;
  status: JukeboxStatus;
  controller: JukeboxController;
  expectedMs: number;
  adding: boolean;
  setAdding: (v: boolean) => void;
  api: JukeboxApi;
  meId: string;
}) {
  const t = useT();
  const cur = s.current;
  return (
    <div className="jb-body">
      {cur ? (
        <div className="jb-now">
          <span className="jb-title" data-testid="jukebox-now">
            {cur.playable ? itemLabel(cur) : t('lounge.jukebox.notShared')}
          </span>
          <span className="jb-muted" data-testid="jukebox-position">
            {clock(expectedMs)}
            {cur.duration_ms ? ` / ${clock(cur.duration_ms)}` : ''}
          </span>
          {status === 'blocked' ? (
            <Button size="sm" onClick={() => void controller.resume()}>
              {t('lounge.jukebox.tapToHear')}
            </Button>
          ) : null}
        </div>
      ) : (
        <p className="jb-muted">{t('lounge.jukebox.empty')}</p>
      )}
      <div className="jb-controls">
        {cur ? (
          <Button
            size="sm"
            variant="ghost"
            aria-pressed={s.my_vote}
            onClick={() =>
              void controller.act((a, c) => (s.my_vote ? a.withdrawVote(c) : a.voteSkip(c)))
            }
          >
            {t('lounge.jukebox.voteSkip', { votes: s.votes, needed: s.votes_needed })}
          </Button>
        ) : null}
        {cur && s.can_control ? (
          <>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void controller.act((a, c) => a.setPaused(c, !s.paused))}
            >
              <Icon name={s.paused ? 'play' : 'pause'} size={16} />{' '}
              {s.paused ? t('lounge.jukebox.resume') : t('lounge.jukebox.pause')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void controller.act((a, c) => a.skipNow(c))}
            >
              {t('lounge.jukebox.skipNow')}
            </Button>
          </>
        ) : null}
        {s.can_queue ? (
          <>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setAdding(!adding)}
              aria-expanded={adding}
            >
              <Icon name="plus" size={16} /> {t('lounge.jukebox.add')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() =>
                void controller.act((a, c) =>
                  a.setMode(c, s.mode === 'synced' ? 'stream' : 'synced'),
                )
              }
              title={t('lounge.jukebox.modeHint')}
            >
              {s.mode === 'synced'
                ? t('lounge.jukebox.modeSynced')
                : t('lounge.jukebox.modeStream')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              aria-pressed={s.i_am_dj}
              disabled={!s.i_am_dj && !!s.dj_id && !s.can_control}
              onClick={() => void controller.act((a, c) => a.setDj(c, s.i_am_dj ? null : meId))}
            >
              {s.i_am_dj ? t('lounge.jukebox.letGo') : t('lounge.jukebox.takeDecks')}
            </Button>
          </>
        ) : null}
        <span className="jb-muted">{t('lounge.jukebox.listeners', { n: s.listeners })}</span>
      </div>
      {adding ? <Picker api={api} controller={controller} /> : null}
      {s.queue.length > 0 ? (
        <ol className="jb-queue" aria-label={t('lounge.jukebox.queueLabel')}>
          {s.queue.map((it) => (
            <li key={it.id} className="jb-row">
              <span>{it.playable ? itemLabel(it) : t('lounge.jukebox.notShared')}</span>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('lounge.jukebox.remove', {
                  title: it.playable ? itemLabel(it) : t('lounge.jukebox.notShared'),
                })}
                onClick={() => void controller.act((a, c) => a.remove(c, it.id))}
              >
                <Icon name="close" size={16} />
              </Button>
            </li>
          ))}
        </ol>
      ) : null}
    </div>
  );
}

function itemLabel(it: JukeboxItem): string {
  return it.artist ? `${it.title ?? ''} – ${it.artist}` : (it.title ?? '');
}

function Picker({ api, controller }: { api: JukeboxApi; controller: JukeboxController }) {
  const t = useT();
  const [q, setQ] = useState('');
  const [rows, setRows] = useState<LibraryTrack[] | null>(null);
  useEffect(() => {
    let dead = false;
    const h = setTimeout(() => {
      api
        .library(q)
        .then((p) => !dead && setRows(p.items))
        .catch(() => !dead && setRows([]));
    }, 200);
    return () => {
      dead = true;
      clearTimeout(h);
    };
  }, [api, q]);
  return (
    <div className="jb-picker">
      <input
        type="search"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder={t('lounge.jukebox.search')}
        aria-label={t('lounge.jukebox.search')}
      />
      {rows && rows.length === 0 ? (
        <p className="jb-muted">{t('lounge.jukebox.noTracks')}</p>
      ) : null}
      <ul className="jb-results">
        {(rows ?? []).map((r) => (
          <li key={r.id} className="jb-row">
            <span>{r.artist ? `${r.title} – ${r.artist}` : r.title}</span>
            <Button
              size="sm"
              variant="ghost"
              aria-label={t('lounge.jukebox.addTrack', { title: r.title })}
              onClick={() => void controller.act((a, c) => a.enqueue(c, [r.id]))}
            >
              <Icon name="plus" size={16} />
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
