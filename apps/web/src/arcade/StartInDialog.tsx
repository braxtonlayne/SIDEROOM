// ARC-UI-011 / D-472, D-478: Play from the Arcade. The member picks where the game should run (the call they are in
// first; then, for a seated game, the server's voice channels before its text channels, and for a board tool the text
// channels first), and the existing ActivityLauncher opens with the game preselected: it launches at once and falls
// back to the shelf if refused. Starting in a voice channel the member is not in joins that call first; a failed join
// launches nothing. Once the game runs, the member goes to that channel, where the Activity has its full-size slot.
import { Button, Icon, Modal, Notice, Spinner } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { httpActivityApi, type ActivityApi } from '../activities/api';
import { useT } from '../i18n';
import { fetchChannels } from '../store/rest';
import { useStore } from '../store/store';
import type { Channel, Guild } from '../store/types';
import { voiceController, type JoinTarget } from '../voice/controller';
import { useVoiceSession } from '../voice/session';

const ActivityLauncher = lazy(() =>
  import('../activities/ActivityLauncher').then((m) => ({ default: m.ActivityLauncher })),
);

/**
 * What Play knows about the game: its id when the card has one, else the name the shelf lists it under; and whether
 * it is played in seats (a call) or is a board tool (as at home in a text channel), D-478.
 */
export interface PlayTarget {
  title: string;
  applicationId?: string;
  shelfName?: string;
  kind?: 'seated' | 'board';
}

const VOICE = 2;
const TEXT = new Set([0, 5]);
/** How long a join may take before Play gives up (the voice controller's own wait is 10 s). */
const JOIN_TIMEOUT_MS = 15_000;

const byPos = (a: Channel, b: Channel) => (a.position ?? 0) - (b.position ?? 0);

/** The voice and the text (and announcement) channels of a server, each in channel-list order. */
export function startableGroups(channels: readonly Channel[]): {
  voice: Channel[];
  text: Channel[];
} {
  return {
    voice: channels.filter((c) => c.type === VOICE).sort(byPos),
    text: channels.filter((c) => TEXT.has(c.type)).sort(byPos),
  };
}

/**
 * The channels of a server where an Activity can start: voice first for a seated game, text first for a board tool
 * (D-478). Never categories.
 */
export function startableChannels(
  channels: readonly Channel[],
  opts: { textFirst?: boolean } = {},
): Channel[] {
  const { voice, text } = startableGroups(channels);
  return opts.textFirst ? [...text, ...voice] : [...voice, ...text];
}

/**
 * Join a call and wait until the voice session is connected to it. Resolves false when the join fails (the channel is
 * full, no Connect, a server error) or takes too long.
 */
export function joinCall(
  target: JoinTarget,
  join: (t: JoinTarget) => void = (t) => voiceController().join(t),
  timeoutMs = JOIN_TIMEOUT_MS,
): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (ok: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      off();
      resolve(ok);
    };
    const check = (s: ReturnType<typeof useVoiceSession.getState>) => {
      if (s.channelId !== target.channelId) return;
      if (s.status === 'connected') finish(true);
      else if (s.status === 'error') finish(false);
    };
    const off = useVoiceSession.subscribe(check);
    const timer = setTimeout(() => finish(false), timeoutMs);
    try {
      join(target);
      check(useVoiceSession.getState());
    } catch {
      finish(false);
    }
  });
}

/** The server's channels from the live store, else (the Arcade opened before the Lounge connected) from REST. */
function useServerChannels(guildId: string, load: (g: string) => Promise<Channel[]>) {
  const ids = useStore((s) => (guildId ? s.channelIdsByGuild[guildId] : undefined));
  const all = useStore((s) => s.channels);
  const live = useMemo(
    () => (ids ?? []).map((id) => all[id]).filter((c): c is Channel => c !== undefined),
    [ids, all],
  );
  const [fetched, setFetched] = useState<{ guild: string; list: Channel[] } | null>(null);
  const [failed, setFailed] = useState(false);
  const need = !!guildId && live.length === 0;
  useEffect(() => {
    if (!need) return;
    let on = true;
    setFailed(false);
    load(guildId)
      .then((list) => on && setFetched({ guild: guildId, list }))
      .catch(() => on && setFailed(true));
    return () => {
      on = false;
    };
  }, [need, guildId, load]);
  if (!need) return { channels: live, loading: false, failed: false };
  const list = fetched?.guild === guildId ? fetched.list : null;
  return { channels: list ?? [], loading: list === null && !failed, failed };
}

export function StartInDialog({
  target,
  guilds,
  onClose,
  api = httpActivityApi,
  loadChannels = fetchChannels,
  join = joinCall,
}: {
  target: PlayTarget | null;
  guilds: Pick<Guild, 'id' | 'name'>[];
  onClose: () => void;
  api?: ActivityApi;
  loadChannels?: (guildId: string) => Promise<Channel[]>;
  /** Joins a call and resolves once connected (false when it could not); tests pass their own. */
  join?: (target: JoinTarget) => Promise<boolean>;
}) {
  const t = useT();
  const navigate = useNavigate();
  // Primitive selectors: an object built in a selector is new on every read and would re-render forever.
  const connected = useVoiceSession((s) => s.status === 'connected');
  const callGuild = useVoiceSession((s) => s.guildId);
  const callChannel = useVoiceSession((s) => s.channelId);
  const callName = useVoiceSession((s) => s.channelName);
  const call =
    connected && callGuild && callChannel
      ? { guildId: callGuild, channelId: callChannel, name: callName ?? '' }
      : null;
  const [picked, setGuild] = useState(guilds[0]?.id ?? '');
  const guildId = guilds.some((g) => g.id === picked) ? picked : (guilds[0]?.id ?? '');
  const { channels, loading, failed } = useServerChannels(guildId, loadChannels);
  const groups = startableGroups(channels.filter((c) => c.id !== call?.channelId));
  const textFirst = target?.kind === 'board';
  const ordered = textFirst
    ? [
        { key: 'text', label: t('arcade.start.text'), list: groups.text },
        { key: 'voice', label: t('arcade.start.voice'), list: groups.voice },
      ]
    : [
        { key: 'voice', label: t('arcade.start.voice'), list: groups.voice },
        { key: 'text', label: t('arcade.start.text'), list: groups.text },
      ];
  const [launch, setLaunch] = useState<{
    guildId: string;
    channelId: string;
    applicationId?: string;
  } | null>(null);
  const [joining, setJoining] = useState<string | null>(null);
  const [joinFailed, setJoinFailed] = useState<string | null>(null);

  const open = target !== null && launch === null;

  const choose = async (gid: string, channel: Pick<Channel, 'id' | 'type' | 'name'>) => {
    if (!target || joining) return;
    setJoinFailed(null);
    // D-478: never launch into a call the member is not in; joining is part of starting.
    if (channel.type === VOICE && channel.id !== call?.channelId) {
      setJoining(channel.id);
      const ok = await join({ guildId: gid, channelId: channel.id, name: channel.name ?? '' });
      setJoining(null);
      if (!ok) {
        setJoinFailed(channel.id);
        return;
      }
    }
    let applicationId = target.applicationId;
    if (!applicationId && target.shelfName) {
      // The shelf lists the approved Activities by their manifest name; without a match the launcher shows the shelf.
      const shelf = await api.shelf().catch(() => null);
      applicationId = shelf?.items.find((i) => i.name === target.shelfName)?.application_id;
    }
    setLaunch({ guildId: gid, channelId: channel.id, applicationId });
  };

  const done = () => {
    setLaunch(null);
    setJoinFailed(null);
    onClose();
  };

  const row = (gid: string, c: Pick<Channel, 'id' | 'type' | 'name'>) => (
    <>
      <button
        type="button"
        className="arc-start-row"
        disabled={joining !== null}
        aria-busy={joining === c.id || undefined}
        onClick={() => void choose(gid, c)}
        aria-label={t(c.type === VOICE ? 'arcade.start.inVoice' : 'arcade.start.in', {
          game: target?.title ?? '',
          channel: c.name ?? '',
        })}
      >
        <Icon name={c.type === VOICE ? 'volume' : 'hash'} size={16} />
        <span>{c.name}</span>
        {joining === c.id && <Spinner size="sm" label={t('arcade.start.joining')} />}
      </button>
      {joinFailed === c.id && (
        <p className="arc-start-error" role="alert">
          {t('arcade.start.joinFailed')}
        </p>
      )}
    </>
  );

  return (
    <>
      <Modal
        open={open}
        onClose={done}
        title={target ? t('arcade.start.title', { game: target.title }) : ''}
        description={t('arcade.start.description')}
      >
        <div className="arc-start">
          {call && (
            <section className="arc-start-group" aria-labelledby="arc-start-call">
              <h3 id="arc-start-call" className="arc-start-label">
                {t('arcade.start.thisCall')}
              </h3>
              {row(call.guildId, { id: call.channelId, type: VOICE, name: call.name })}
            </section>
          )}
          {guilds.length === 0 ? (
            <p className="arc-dim">{t('arcade.cartridges.none')}</p>
          ) : (
            <section className="arc-start-group" aria-labelledby="arc-start-server">
              {/* One server: the group still has its name for screen readers, but no double label on screen. */}
              <h3
                id="arc-start-server"
                className={guilds.length === 1 ? 'arc-sr-only' : 'arc-start-label'}
              >
                {t('arcade.start.channels')}
              </h3>
              {guilds.length > 1 && (
                <label className="arc-field">
                  <span>{t('arcade.cartridges.server')}</span>
                  <select value={guildId} onChange={(e) => setGuild(e.target.value)}>
                    {guilds.map((g) => (
                      <option key={g.id} value={g.id}>
                        {g.name}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {loading && <Spinner label={t('arcade.start.loading')} />}
              {failed && <Notice variant="danger" title={t('arcade.start.failed')} />}
              {!loading && !failed && groups.voice.length + groups.text.length === 0 && (
                <p className="arc-dim">{t('arcade.start.none')}</p>
              )}
              {ordered
                .filter((g) => g.list.length > 0)
                .map((g) => (
                  <ul key={g.key} className="arc-start-list" aria-label={g.label}>
                    {g.list.map((c) => (
                      <li key={c.id}>{row(guildId, c)}</li>
                    ))}
                  </ul>
                ))}
            </section>
          )}
          <div className="arc-form-actions">
            <Button variant="secondary" onClick={done}>
              {t('arcade.cartridges.cancel')}
            </Button>
          </div>
        </div>
      </Modal>
      {launch && (
        <Suspense fallback={null}>
          <ActivityLauncher
            open
            channelId={launch.channelId}
            applicationId={launch.applicationId}
            api={api}
            onClose={done}
            onStarted={() =>
              void navigate({
                to: '/channels/$guildId/$channelId',
                params: { guildId: launch.guildId, channelId: launch.channelId },
              })
            }
          />
        </Suspense>
      )}
    </>
  );
}
