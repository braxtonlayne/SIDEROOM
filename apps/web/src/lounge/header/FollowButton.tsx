import { Button, Icon, Modal, Notice, Tooltip } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import * as rest from '../../store/rest';
import './header.css';

interface Group {
  id: string;
  name: string;
  channels: { id: string; name: string }[];
}

/** Text (0) and announcement (5) channels of every server the viewer is in; the server checks Manage Webhooks. */
async function loadTargets(): Promise<Group[]> {
  const guilds = await rest.fetchGuilds();
  const groups = await Promise.all(
    guilds.map(async (g) => {
      const channels = await rest.fetchChannels(g.id).catch(() => []);
      return {
        id: g.id,
        name: g.name,
        channels: channels
          .filter((c) => c.type === 0 || c.type === 5)
          .sort((a, b) => (a.position ?? 0) - (b.position ?? 0))
          .map((c) => ({ id: c.id, name: c.name ?? c.id })),
      };
    }),
  );
  return groups.filter((g) => g.channels.length > 0);
}

/**
 * Follow button of an announcement channel header (G-30, LNG-CHN-004): pick one of your channels and
 * `POST /channels/{id}/followers {webhook_channel_id}` makes it receive everything published here.
 */
export function FollowButton({ channelId }: { channelId: string }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [groups, setGroups] = useState<Group[] | null>(null);
  const [target, setTarget] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  useEffect(() => {
    if (!open || groups) return;
    let live = true;
    loadTargets()
      .then((g) => live && setGroups(g))
      .catch(() => live && setError(t('lounge.followButton.loadError')));
    return () => {
      live = false;
    };
  }, [open, groups, t]);

  const close = () => {
    setOpen(false);
    setError(null);
    setDone(null);
  };

  const follow = async () => {
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      await apiFetch(`/channels/${channelId}/followers`, {
        method: 'POST',
        json: { webhook_channel_id: target },
      });
      const name = groups?.flatMap((g) => g.channels).find((c) => c.id === target)?.name ?? target;
      setDone(name);
    } catch (e) {
      setError(
        e instanceof ApiError && e.status === 403
          ? t('lounge.followButton.forbidden')
          : t('lounge.followButton.failed'),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Tooltip content={t('lounge.followButton.tooltip')}>
        <button
          type="button"
          className="lng-icon-btn lng-follow-btn"
          aria-label={t('lounge.followButton.label')}
          onClick={() => setOpen(true)}
        >
          <Icon name="bell" size={20} />
          <span className="lng-follow-text">{t('lounge.followButton.label')}</span>
        </button>
      </Tooltip>
      <Modal
        open={open}
        onClose={close}
        size="sm"
        title={t('lounge.followButton.title')}
        description={t('lounge.followButton.description')}
        footer={
          <>
            <Button variant="ghost" onClick={close}>
              {done ? t('lounge.followButton.close') : t('lounge.followButton.cancel')}
            </Button>
            {!done && (
              <Button loading={busy} disabled={!target} onClick={() => void follow()}>
                {t('lounge.followButton.confirm')}
              </Button>
            )}
          </>
        }
      >
        {done ? (
          <Notice variant="success">{t('lounge.followButton.done', { channel: done })}</Notice>
        ) : (
          <>
            {error && <Notice variant="danger">{error}</Notice>}
            {!groups && !error && <p role="status">{t('lounge.followButton.loading')}</p>}
            {groups && (
              <label className="lng-follow-field">
                <span>{t('lounge.followButton.channel')}</span>
                <select
                  className="lng-follow-select"
                  value={target}
                  onChange={(e) => setTarget(e.target.value)}
                >
                  <option value="">{t('lounge.followButton.choose')}</option>
                  {groups.map((g) => (
                    <optgroup key={g.id} label={g.name}>
                      {g.channels.map((c) => (
                        <option key={c.id} value={c.id}>
                          #{c.name}
                        </option>
                      ))}
                    </optgroup>
                  ))}
                </select>
              </label>
            )}
          </>
        )}
      </Modal>
    </>
  );
}
