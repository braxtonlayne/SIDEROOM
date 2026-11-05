// ARC-UI-011 / D-472: Play from the Arcade. The member picks where the game should run (the call they are in first,
// then their servers' voice channels, then the text channels the composer's + menu offers), and the existing
// ActivityLauncher opens with the game preselected: it launches at once and falls back to the shelf if refused.
// Once the game is running, the member is taken to that channel, where the Activity panel shows it.
import { Button, Icon, Modal, Notice, Spinner } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { httpActivityApi, type ActivityApi } from '../activities/api';
import { useT } from '../i18n';
import { fetchChannels } from '../store/rest';
import { useStore } from '../store/store';
import type { Channel, Guild } from '../store/types';
import { useVoiceSession } from '../voice/session';

const ActivityLauncher = lazy(() =>
  import('../activities/ActivityLauncher').then((m) => ({ default: m.ActivityLauncher })),
);

/** What Play knows about the game: its id when the card has one, else the name the shelf lists it under. */
export interface PlayTarget {
  title: string;
  applicationId?: string;
  shelfName?: string;
}

const VOICE = 2;
const TEXT = new Set([0, 5]);

/** The channels of a server where an Activity can start: voice first, then text and announcement channels. */
export function startableChannels(channels: readonly Channel[]): Channel[] {
  const byPos = (a: Channel, b: Channel) => (a.position ?? 0) - (b.position ?? 0);
  return [
    ...channels.filter((c) => c.type === VOICE).sort(byPos),
    ...channels.filter((c) => TEXT.has(c.type)).sort(byPos),
  ];
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
}: {
  target: PlayTarget | null;
  guilds: Pick<Guild, 'id' | 'name'>[];
  onClose: () => void;
  api?: ActivityApi;
  loadChannels?: (guildId: string) => Promise<Channel[]>;
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
  const list = startableChannels(channels);
  const [launch, setLaunch] = useState<{
    guildId: string;
    channelId: string;
    applicationId?: string;
  } | null>(null);

  const open = target !== null && launch === null;

  const choose = async (gid: string, channelId: string) => {
    if (!target) return;
    let applicationId = target.applicationId;
    if (!applicationId && target.shelfName) {
      // The shelf lists the approved Activities by their manifest name; without a match the launcher shows the shelf.
      const shelf = await api.shelf().catch(() => null);
      applicationId = shelf?.items.find((i) => i.name === target.shelfName)?.application_id;
    }
    setLaunch({ guildId: gid, channelId, applicationId });
  };

  const done = () => {
    setLaunch(null);
    onClose();
  };

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
              <button
                type="button"
                className="arc-start-row"
                onClick={() => void choose(call.guildId, call.channelId)}
                aria-label={t('arcade.start.in', {
                  game: target?.title ?? '',
                  channel: call.name,
                })}
              >
                <Icon name="volume" size={16} />
                <span>{call.name}</span>
              </button>
            </section>
          )}
          {guilds.length === 0 ? (
            <p className="arc-dim">{t('arcade.cartridges.none')}</p>
          ) : (
            <section className="arc-start-group" aria-labelledby="arc-start-server">
              <h3 id="arc-start-server" className="arc-start-label">
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
              {!loading && !failed && list.length === 0 && (
                <p className="arc-dim">{t('arcade.start.none')}</p>
              )}
              <ul className="arc-start-list">
                {list.map((c) => (
                  <li key={c.id}>
                    <button
                      type="button"
                      className="arc-start-row"
                      onClick={() => void choose(guildId, c.id)}
                      aria-label={t('arcade.start.in', {
                        game: target?.title ?? '',
                        channel: c.name ?? '',
                      })}
                    >
                      <Icon name={c.type === VOICE ? 'volume' : 'hash'} size={16} />
                      <span>{c.name}</span>
                    </button>
                  </li>
                ))}
              </ul>
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
