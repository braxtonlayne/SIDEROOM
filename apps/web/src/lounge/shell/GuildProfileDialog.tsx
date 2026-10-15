import { Avatar, Button, Modal, TextArea, TextInput } from '@sideroom/ui';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { readAsDataUri } from '../../settings/user/imageCrop';
import { useStore } from '../../store';
import { cdnUrl } from '../../store/cdn';
import { useCurrentUser, useGuild } from '../data';

const BIO_MAX = 190;
const NICK_MAX = 32;

/** The caller's own member object (`GET /users/@me/guilds/{id}/member`), the fields a per-server profile has. */
interface OwnMember {
  nick?: string | null;
  avatar?: string | null;
  banner?: string | null;
  bio?: string | null;
  roles?: string[];
}

/** `undefined` = leave as is, `null` = clear, string = a new image as a data URI. */
type ImageEdit = string | null | undefined;

function ImagePick({
  label,
  current,
  edit,
  onEdit,
  onError,
  round,
}: {
  label: string;
  current: string | null;
  edit: ImageEdit;
  onEdit: (v: ImageEdit) => void;
  onError: (msg: string) => void;
  round?: boolean;
}) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const shown = edit === undefined ? current : edit;
  return (
    <div className="lng-gp-image">
      <span className="lng-gp-image-label">{label}</span>
      <div className="lng-gp-image-row">
        {round ? (
          <Avatar name={label} src={shown} size={56} />
        ) : (
          <div
            className="lng-gp-banner"
            aria-hidden="true"
            style={shown ? { backgroundImage: `url(${shown})` } : undefined}
          />
        )}
        <input
          ref={input}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif"
          hidden
          aria-label={t('lounge.guildProfile.fileFor', { label })}
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = '';
            if (!file) return;
            readAsDataUri(file).then(onEdit, () => onError(t('lounge.guildProfile.readError')));
          }}
        />
        <Button size="sm" variant="secondary" onClick={() => input.current?.click()}>
          {t('lounge.guildProfile.change')}
        </Button>
        {shown && (
          <Button size="sm" variant="ghost" onClick={() => onEdit(null)}>
            {t('lounge.guildProfile.remove')}
          </Button>
        )}
      </div>
    </div>
  );
}

/**
 * Edit Per-server Profile (LNG-RAIL-012, LNG-PRF-011): nickname, avatar, banner and bio that apply in one server
 * only (`PATCH /guilds/{id}/members/@me`; images are data URIs, `null` clears a field).
 */
export function GuildProfileDialog({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const t = useT();
  const guild = useGuild(guildId);
  const me = useCurrentUser();
  const [loaded, setLoaded] = useState(false);
  const [nick, setNick] = useState('');
  const [bio, setBio] = useState('');
  const [avatar, setAvatar] = useState<string | null>(null);
  const [banner, setBanner] = useState<string | null>(null);
  const [avatarEdit, setAvatarEdit] = useState<ImageEdit>(undefined);
  const [bannerEdit, setBannerEdit] = useState<ImageEdit>(undefined);
  const [roles, setRoles] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    apiFetch<OwnMember>(`/users/@me/guilds/${encodeURIComponent(guildId)}/member`, { retries: 0 })
      .then((m) => {
        if (!live) return;
        setNick(m.nick ?? '');
        setBio(m.bio ?? '');
        setAvatar(cdnUrl('avatars', me.id, m.avatar, { size: 128 }));
        setBanner(cdnUrl('avatars', me.id, m.banner, { size: 600 }));
        setRoles(m.roles ?? []);
        setLoaded(true);
      })
      .catch(() => live && setLoaded(true));
    return () => {
      live = false;
    };
  }, [guildId, me.id]);

  if (!guild) return null;

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const body: Record<string, string | null> = {
      nick: nick.trim() || null,
      bio: bio.trim() || null,
    };
    if (avatarEdit !== undefined) body.avatar = avatarEdit;
    if (bannerEdit !== undefined) body.banner = bannerEdit;
    try {
      const m = await apiFetch<OwnMember>(`/guilds/${encodeURIComponent(guildId)}/members/@me`, {
        method: 'PATCH',
        json: body,
        retries: 0,
      });
      useStore.getState().applyDispatch('GUILD_MEMBER_UPDATE', {
        guild_id: guildId,
        user: { id: me.id },
        nick: m.nick ?? (body.nick as string | null),
        roles: m.roles ?? roles,
        avatar: m.avatar ?? null,
        banner: m.banner ?? null,
        bio: m.bio ?? (body.bio as string | null),
      });
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError && err.message ? err.message : t('lounge.guildProfile.saveError'),
      );
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.guildProfile.title')}
      description={t('lounge.guildProfile.subtitle', { name: guild.name })}
    >
      <form className="lng-form" onSubmit={(e) => void save(e)}>
        <TextInput
          label={t('lounge.guildProfile.nick')}
          value={nick}
          maxLength={NICK_MAX}
          disabled={!loaded}
          onChange={(e) => setNick(e.target.value)}
        />
        <TextArea
          label={t('lounge.guildProfile.bio')}
          value={bio}
          maxLength={BIO_MAX}
          rows={3}
          disabled={!loaded}
          hint={t('lounge.guildProfile.bioCount', { used: bio.length, max: BIO_MAX })}
          onChange={(e) => setBio(e.target.value)}
        />
        <ImagePick
          label={t('lounge.guildProfile.avatar')}
          current={avatar}
          edit={avatarEdit}
          onEdit={setAvatarEdit}
          onError={setError}
          round
        />
        <ImagePick
          label={t('lounge.guildProfile.banner')}
          current={banner}
          edit={bannerEdit}
          onEdit={setBannerEdit}
          onError={setError}
        />
        {error && (
          <p role="alert" className="lng-failed">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.guildProfile.cancel')}
          </Button>
          <Button type="submit" loading={busy} disabled={!loaded}>
            {t('lounge.guildProfile.save')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
