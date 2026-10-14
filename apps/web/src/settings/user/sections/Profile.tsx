import {
  Avatar,
  Button,
  ColorPicker,
  Notice,
  SegmentedControl,
  Select,
  TextArea,
  TextInput,
  useToast,
} from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { t, useT } from '../../../i18n';
import { cdnUrl } from '../../../store';
import {
  errorText,
  getMyMember,
  getOwnProfile,
  getPresence,
  listGuilds,
  patchMyNick,
  patchSelf,
  useResource,
  type GuildLite,
  type OwnProfile,
  type ProfilePatch,
} from '../api';
import { ImageCropDialog } from '../ImageCropDialog';
import { imageProblem, readAsDataUri } from '../imageCrop';
import { Card, Loaded, SectionTitle, Unavailable } from '../parts';

export const BIO_MAX = 190;
export const PRONOUNS_MAX = 40;
export const STATUS_MAX = 128;
export const DISPLAY_NAME_MAX = 32;
export const NICK_MAX = 32;

export interface ProfileDraft {
  displayName: string;
  pronouns: string;
  bio: string;
  /** `#rrggbb` or null for none. */
  accent: string | null;
  status: string;
  /** A new picture as a data URI, null to remove it, absent to leave it alone. */
  avatar?: string | null;
  banner?: string | null;
}

const EMPTY: ProfileDraft = { displayName: '', pronouns: '', bio: '', accent: null, status: '' };

const toHex = (n: number | null | undefined) =>
  typeof n === 'number' ? `#${n.toString(16).padStart(6, '0')}` : null;
const fromHex = (h: string) => parseInt(h.slice(1), 16);

export function validateProfile(d: ProfileDraft): Partial<Record<keyof ProfileDraft, string>> {
  const errors: Partial<Record<keyof ProfileDraft, string>> = {};
  if ([...d.displayName].length > DISPLAY_NAME_MAX)
    errors.displayName = t('settings.profile.displayNameMax', { max: DISPLAY_NAME_MAX });
  if ([...d.bio].length > BIO_MAX) errors.bio = t('settings.profile.bioMax', { max: BIO_MAX });
  if ([...d.pronouns].length > PRONOUNS_MAX)
    errors.pronouns = t('settings.profile.pronounsMax', { max: PRONOUNS_MAX });
  if ([...d.status].length > STATUS_MAX)
    errors.status = t('settings.profile.statusMax', { max: STATUS_MAX });
  return errors;
}

/** Only the fields that changed, in the shape PATCH /users/@me takes (`null` clears). */
export function diffProfile(saved: ProfileDraft, draft: ProfileDraft): ProfilePatch {
  const patch: ProfilePatch = {};
  if (draft.displayName !== saved.displayName)
    patch.global_name = draft.displayName.trim() === '' ? null : draft.displayName.trim();
  if (draft.bio !== saved.bio) patch.bio = draft.bio.trim() === '' ? null : draft.bio;
  if (draft.pronouns !== saved.pronouns)
    patch.pronouns = draft.pronouns.trim() === '' ? null : draft.pronouns;
  if (draft.accent !== saved.accent)
    patch.accent_color = draft.accent ? fromHex(draft.accent) : null;
  if (draft.status !== saved.status)
    patch.custom_status = draft.status.trim() === '' ? null : { text: draft.status };
  if (draft.avatar !== undefined) patch.avatar = draft.avatar;
  if (draft.banner !== undefined) patch.banner = draft.banner;
  return patch;
}

export interface ProfileForm {
  loading: boolean;
  loadError: string | null;
  profile: OwnProfile | null;
  draft: ProfileDraft;
  setDraft: (patch: Partial<ProfileDraft>) => void;
  dirty: boolean;
  saveError: string | null;
  save: () => Promise<void>;
  reset: () => void;
}

/** Lives in the settings root so edits survive switching sections; loads only once `active` is first true. */
export function useProfileForm(active: boolean): ProfileForm {
  const [state, setState] = useState<'idle' | 'loading' | 'ready' | 'error'>('idle');
  const [loadError, setLoadError] = useState<string | null>(null);
  const [profile, setProfile] = useState<OwnProfile | null>(null);
  const [saved, setSaved] = useState<ProfileDraft>(EMPTY);
  const [draft, setDraftState] = useState<ProfileDraft>(EMPTY);
  const [saveError, setSaveError] = useState<string | null>(null);
  const started = useRef(false);

  useEffect(() => {
    if (!active || started.current) return;
    started.current = true;
    setState('loading');
    Promise.all([getOwnProfile(), getPresence()])
      .then(([p, presence]) => {
        const d: ProfileDraft = {
          displayName: p.user.global_name ?? '',
          pronouns: p.user_profile.pronouns ?? '',
          bio: p.user_profile.bio ?? '',
          accent: toHex(p.user_profile.accent_color),
          status: presence.custom_status?.text ?? '',
        };
        setProfile(p);
        setSaved(d);
        setDraftState(d);
        setState('ready');
      })
      .catch((e: unknown) => {
        setLoadError(errorText(e));
        setState('error');
      });
  }, [active]);

  const dirty = state === 'ready' && JSON.stringify(saved) !== JSON.stringify(draft);

  const save = useCallback(async () => {
    setSaveError(null);
    const errors = validateProfile(draft);
    const first = Object.values(errors)[0];
    if (first) {
      setSaveError(first);
      throw new Error(first);
    }
    const patch = diffProfile(saved, draft);
    try {
      const res = await patchSelf(patch);
      // Pictures are now on the server: the preview switches from the pending upload to the stored hash.
      const next: ProfileDraft = { ...draft };
      delete next.avatar;
      delete next.banner;
      setSaved(next);
      setDraftState(next);
      if (res?.user) setProfile((p) => (p ? { ...p, user: { ...p.user, ...res.user } } : p));
    } catch (e) {
      const msg = errorText(e);
      setSaveError(msg);
      throw e;
    }
  }, [draft, saved]);

  return {
    loading: state === 'idle' || state === 'loading',
    loadError,
    profile,
    draft,
    setDraft: (patch) => setDraftState((d) => ({ ...d, ...patch })),
    dirty,
    saveError,
    save,
    reset: () => {
      setDraftState(saved);
      setSaveError(null);
    },
  };
}

function PreviewCard({ form }: { form: ProfileForm }) {
  const t = useT();
  const u = form.profile?.user;
  const name = form.draft.displayName.trim() || u?.username || t('settings.profile.you');
  const accent = form.draft.accent;
  const avatar =
    form.draft.avatar !== undefined
      ? form.draft.avatar
      : u
        ? cdnUrl('avatars', u.id, u.avatar, { size: 128 })
        : null;
  const banner =
    form.draft.banner !== undefined
      ? form.draft.banner
      : u
        ? cdnUrl('avatars', u.id, u.banner, { size: 512 })
        : null;
  return (
    <aside className="us-preview" aria-label={t('settings.profile.previewAria')}>
      <div
        className="us-preview-banner"
        style={
          banner
            ? { backgroundImage: `url(${banner})`, backgroundSize: 'cover' }
            : accent
              ? { background: accent }
              : undefined
        }
      />
      <div className="us-preview-body">
        <Avatar name={name} src={avatar} size={80} className="us-preview-avatar" />
        <p className="us-preview-name">{name}</p>
        <p className="us-preview-sub">
          {u?.username}
          {form.draft.pronouns && ` · ${form.draft.pronouns}`}
        </p>
        {form.draft.status && <p className="us-preview-sub">{form.draft.status}</p>}
        {form.draft.bio && (
          <>
            <h3>{t('settings.profile.aboutMe')}</h3>
            <p>{form.draft.bio}</p>
          </>
        )}
      </div>
    </aside>
  );
}

type Pick = { kind: 'avatar' | 'banner'; file: File };

/** Avatar and banner: choose a file, crop it, and the result waits in the draft until Save changes. */
function MediaCard({ form }: { form: ProfileForm }) {
  const t = useT();
  const avatarInput = useRef<HTMLInputElement>(null);
  const bannerInput = useRef<HTMLInputElement>(null);
  const [pick, setPick] = useState<Pick | null>(null);
  const [error, setError] = useState<string | null>(null);
  const u = form.profile?.user;
  const hasAvatar = form.draft.avatar !== undefined ? form.draft.avatar !== null : !!u?.avatar;
  const hasBanner = form.draft.banner !== undefined ? form.draft.banner !== null : !!u?.banner;

  const onFile = async (kind: Pick['kind'], file: File | undefined) => {
    if (!file) return;
    const problem = imageProblem(file);
    if (problem) return setError(problem);
    setError(null);
    if (file.type === 'image/gif') {
      // A canvas would flatten the animation: the server centres and sizes it.
      try {
        form.setDraft({ [kind]: await readAsDataUri(file) });
      } catch (e) {
        setError(e instanceof Error ? e.message : t('settings.profile.readError'));
      }
      return;
    }
    setPick({ kind, file });
  };

  return (
    <Card
      id="us-media"
      title={t('settings.profile.mediaTitle')}
      description={t('settings.profile.mediaDesc')}
    >
      <div className="us-stack">
        <div className="us-actions">
          <Button variant="secondary" onClick={() => avatarInput.current?.click()}>
            {t('settings.profile.changeAvatar')}
          </Button>
          {hasAvatar && (
            <Button variant="ghost" onClick={() => form.setDraft({ avatar: null })}>
              {t('settings.profile.removeAvatar')}
            </Button>
          )}
          <Button variant="secondary" onClick={() => bannerInput.current?.click()}>
            {t('settings.profile.changeBanner')}
          </Button>
          {hasBanner && (
            <Button variant="ghost" onClick={() => form.setDraft({ banner: null })}>
              {t('settings.profile.removeBanner')}
            </Button>
          )}
        </div>
        <input
          ref={avatarInput}
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          className="us-sr"
          tabIndex={-1}
          aria-label={t('settings.profile.avatarFile')}
          onChange={(e) => {
            void onFile('avatar', e.target.files?.[0]);
            e.target.value = '';
          }}
        />
        <input
          ref={bannerInput}
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          className="us-sr"
          tabIndex={-1}
          aria-label={t('settings.profile.bannerFile')}
          onChange={(e) => {
            void onFile('banner', e.target.files?.[0]);
            e.target.value = '';
          }}
        />
        {error && (
          <Notice variant="danger" title={t('settings.profile.imageError')}>
            {error}
          </Notice>
        )}
        <p className="us-muted">{t('settings.profile.mediaNote')}</p>
      </div>
      <ImageCropDialog
        open={pick?.kind === 'avatar'}
        file={pick?.kind === 'avatar' ? pick.file : null}
        title={t('settings.profile.cropAvatar')}
        aspect={1}
        outWidth={512}
        onCancel={() => setPick(null)}
        onApply={(uri) => {
          form.setDraft({ avatar: uri });
          setPick(null);
        }}
      />
      <ImageCropDialog
        open={pick?.kind === 'banner'}
        file={pick?.kind === 'banner' ? pick.file : null}
        title={t('settings.profile.cropBanner')}
        aspect={2.5}
        outWidth={1200}
        onCancel={() => setPick(null)}
        onApply={(uri) => {
          form.setDraft({ banner: uri });
          setPick(null);
        }}
      />
    </Card>
  );
}

/** One server's nickname. Saved on its own button (not the page bar) since it is a different resource. */
function ServerNick({ guild, userId }: { guild: GuildLite; userId: string }) {
  const t = useT();
  const toast = useToast();
  const member = useResource(() => getMyMember(guild.id, userId));
  const [nick, setNick] = useState('');
  const [seen, setSeen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const current = member.data?.nick ?? '';
  // Adopt the loaded value once (and again after a successful save).
  if (member.data && seen !== current) {
    setSeen(current);
    setNick(current);
  }
  const over = [...nick].length > NICK_MAX;
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await patchMyNick(guild.id, userId, nick.trim() === '' ? null : nick.trim());
      member.setData(res);
      toast.show({
        title: t('settings.profile.nickUpdated', { name: guild.name }),
        duration: 2500,
      });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Loaded res={member} what={t('settings.profile.membershipWhat')}>
      {() => (
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            if (!over) void save();
          }}
        >
          <TextInput
            label={t('settings.profile.nickLabel')}
            value={nick}
            onChange={(e) => setNick(e.target.value)}
            error={over ? t('settings.profile.nickMax', { max: NICK_MAX }) : undefined}
            hint={t('settings.profile.nickHint')}
          />
          <div className="us-actions">
            <Button type="submit" loading={busy} disabled={over || nick === current}>
              {t('settings.profile.saveNick')}
            </Button>
          </div>
          {error && (
            <Notice variant="danger" title={t('settings.profile.nickError')}>
              {error}
            </Notice>
          )}
        </form>
      )}
    </Loaded>
  );
}

function ServerProfiles({ userId }: { userId: string | undefined }) {
  const t = useT();
  const guilds = useResource(listGuilds);
  const [picked, setPicked] = useState<string | null>(null);
  return (
    <Card
      id="us-server-profiles"
      title={t('settings.profile.serversTitle')}
      description={t('settings.profile.serversDesc')}
    >
      <Loaded res={guilds} what={t('settings.profile.serversWhat')}>
        {(rows) => {
          if (rows.length === 0)
            return <p className="us-muted">{t('settings.profile.noServers')}</p>;
          const guild = rows.find((g) => g.id === picked) ?? rows[0];
          return (
            <div className="us-stack">
              <label className="us-muted" id="us-sp-label">
                {t('settings.profile.server')}
              </label>
              <Select
                aria-labelledby="us-sp-label"
                options={rows.map((g) => ({ value: g.id, label: g.name }))}
                value={guild?.id ?? null}
                onValueChange={setPicked}
              />
              {guild && userId && <ServerNick key={guild.id} guild={guild} userId={userId} />}
              <Unavailable>{t('settings.profile.serversUnavailable')}</Unavailable>
            </div>
          );
        }}
      </Loaded>
    </Card>
  );
}

export function ProfileSection({ form }: { form: ProfileForm }) {
  const t = useT();
  const errors = validateProfile(form.draft);
  const bioLen = [...form.draft.bio].length;
  const [tab, setTab] = useState('main');
  return (
    <>
      <SectionTitle>{t('settings.profile.title')}</SectionTitle>
      {form.loading ? (
        <p className="us-muted" role="status">
          {t('settings.profile.loading')}
        </p>
      ) : form.loadError ? (
        <Notice variant="danger" title={t('settings.profile.loadError')}>
          {form.loadError}
        </Notice>
      ) : (
        <>
          <SegmentedControl
            label={t('settings.profile.tabs')}
            value={tab}
            onValueChange={setTab}
            options={[
              { value: 'main', label: t('settings.profile.main') },
              { value: 'servers', label: t('settings.profile.serversTitle') },
            ]}
          />
          {tab === 'servers' ? (
            <ServerProfiles userId={form.profile?.user.id} />
          ) : (
            <div className="us-split">
              <div>
                <Card id="us-profile" title={t('settings.profile.main')}>
                  <div className="us-stack">
                    <TextInput
                      label={t('settings.profile.displayName')}
                      value={form.draft.displayName}
                      onChange={(e) => form.setDraft({ displayName: e.target.value })}
                      error={errors.displayName}
                      hint={t('settings.profile.displayNameHint')}
                    />
                    <TextInput
                      label={t('settings.profile.pronouns')}
                      value={form.draft.pronouns}
                      onChange={(e) => form.setDraft({ pronouns: e.target.value })}
                      error={errors.pronouns}
                    />
                    <div>
                      <TextArea
                        label={t('settings.profile.aboutMe')}
                        value={form.draft.bio}
                        onChange={(e) => form.setDraft({ bio: e.target.value })}
                        rows={4}
                        error={errors.bio}
                      />
                      <p
                        className="us-counter"
                        data-over={bioLen > BIO_MAX || undefined}
                        aria-live="polite"
                      >
                        {bioLen}/{BIO_MAX}
                      </p>
                    </div>
                    <TextInput
                      label={t('settings.profile.customStatus')}
                      value={form.draft.status}
                      onChange={(e) => form.setDraft({ status: e.target.value })}
                      error={errors.status}
                    />
                    <div>
                      <ColorPicker
                        label={t('settings.profile.accent')}
                        value={form.draft.accent ?? '#c2b08f'}
                        onValueChange={(hex) => form.setDraft({ accent: hex })}
                      />
                      {form.draft.accent && (
                        <Button
                          variant="link"
                          size="sm"
                          onClick={() => form.setDraft({ accent: null })}
                        >
                          {t('settings.profile.clearAccent')}
                        </Button>
                      )}
                    </div>
                  </div>
                </Card>
                <MediaCard form={form} />
                <Card
                  id="us-profile-extras"
                  title={t('settings.profile.extrasTitle')}
                  description={t('settings.profile.extrasDesc')}
                >
                  <Unavailable>{t('settings.profile.extrasUnavailable')}</Unavailable>
                </Card>
                {form.saveError && (
                  <Notice variant="danger" title={t('settings.profile.saveError')}>
                    {form.saveError}
                  </Notice>
                )}
              </div>
              <PreviewCard form={form} />
            </div>
          )}
        </>
      )}
    </>
  );
}
