import {
  Avatar,
  Button,
  ColorPicker,
  Modal,
  Select,
  Switch,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { readAsDataUri } from '../../settings/user/imageCrop';
import { colorOfHex, hexOfColor } from '../../lib/serverTag';
import { useStore } from '../../store';
import { cdnUrl } from '../../store/cdn';
import { useCurrentUser, useGuild } from '../data';

const BIO_MAX = 190;
const NICK_MAX = 32;
const PRONOUNS_MAX = 40;
/** Profile effects the server accepts today (the profile catalogue; decorations are not offered yet). */
const EFFECTS = [
  ['sparkle', 'lounge.guildProfile.effectSparkle'],
  ['aurora', 'lounge.guildProfile.effectAurora'],
  ['snow', 'lounge.guildProfile.effectSnow'],
  ['embers', 'lounge.guildProfile.effectEmbers'],
] as const;
const DEFAULT_THEME: [number, number] = [0x5865f2, 0x99aab5];

/** The caller's own member object (`GET /users/@me/guilds/{id}/member`), the fields a per-server profile has. */
interface OwnMember {
  nick?: string | null;
  avatar?: string | null;
  banner?: string | null;
  bio?: string | null;
  roles?: string[];
  /** Per-server extras (G-52); absent or null means the account's own. */
  pronouns?: string | null;
  theme_colors?: [number, number] | null;
  profile_effect?: string | null;
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
  const [pronouns, setPronouns] = useState('');
  const [theme, setTheme] = useState<[number, number] | null>(null);
  const [effect, setEffect] = useState('');
  /** What the server held when the dialog opened: only changed extras are sent. */
  const [initial, setInitial] = useState<{
    pronouns: string;
    theme: [number, number] | null;
    effect: string;
  }>({ pronouns: '', theme: null, effect: '' });
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
        setPronouns(m.pronouns ?? '');
        setTheme(m.theme_colors ?? null);
        setEffect(m.profile_effect ?? '');
        setInitial({
          pronouns: m.pronouns ?? '',
          theme: m.theme_colors ?? null,
          effect: m.profile_effect ?? '',
        });
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
    const body: Record<string, string | number[] | null> = {
      nick: nick.trim() || null,
      bio: bio.trim() || null,
    };
    if (pronouns.trim() !== initial.pronouns) body.pronouns = pronouns.trim() || null;
    if (JSON.stringify(theme) !== JSON.stringify(initial.theme)) body.theme_colors = theme;
    if (effect !== initial.effect) body.profile_effect = effect || null;
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
        pronouns: m.pronouns ?? null,
        theme_colors: m.theme_colors ?? null,
        profile_effect: m.profile_effect ?? null,
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
        <TextInput
          label={t('lounge.guildProfile.pronouns')}
          value={pronouns}
          maxLength={PRONOUNS_MAX}
          disabled={!loaded}
          onChange={(e) => setPronouns(e.target.value)}
        />
        <label className="lng-field-label" htmlFor="gp-effect">
          {t('lounge.guildProfile.effect')}
        </label>
        <Select
          id="gp-effect"
          value={effect}
          disabled={!loaded}
          onValueChange={setEffect}
          options={[
            { value: '', label: t('lounge.guildProfile.effectNone') },
            ...EFFECTS.map(([value, key]) => ({ value, label: t(key) })),
          ]}
        />
        <Switch
          label={t('lounge.guildProfile.theme')}
          description={t('lounge.guildProfile.themeHint')}
          checked={theme !== null}
          disabled={!loaded}
          onChange={(e) => setTheme(e.target.checked ? DEFAULT_THEME : null)}
        />
        {theme && (
          <div className="lng-gp-theme">
            <ColorPicker
              label={t('lounge.guildProfile.themePrimary')}
              value={hexOfColor(theme[0])}
              onValueChange={(hex) => setTheme([colorOfHex(hex), theme[1]])}
            />
            <ColorPicker
              label={t('lounge.guildProfile.themeAccent')}
              value={hexOfColor(theme[1])}
              onValueChange={(hex) => setTheme([theme[0], colorOfHex(hex)])}
            />
          </div>
        )}
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
