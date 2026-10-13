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
    errors.displayName = `Display names can be at most ${DISPLAY_NAME_MAX} characters.`;
  if ([...d.bio].length > BIO_MAX) errors.bio = `About me can be at most ${BIO_MAX} characters.`;
  if ([...d.pronouns].length > PRONOUNS_MAX)
    errors.pronouns = `Pronouns can be at most ${PRONOUNS_MAX} characters.`;
  if ([...d.status].length > STATUS_MAX)
    errors.status = `Custom status can be at most ${STATUS_MAX} characters.`;
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
  const u = form.profile?.user;
  const name = form.draft.displayName.trim() || u?.username || 'You';
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
    <aside className="us-preview" aria-label="Profile preview">
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
            <h3>About me</h3>
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
        setError(e instanceof Error ? e.message : 'That file could not be read.');
      }
      return;
    }
    setPick({ kind, file });
  };

  return (
    <Card
      id="us-media"
      title="Avatar and banner"
      description="PNG, JPEG, WebP or GIF up to 8 MiB. GIFs keep their animation and are centred by the server."
    >
      <div className="us-stack">
        <div className="us-actions">
          <Button variant="secondary" onClick={() => avatarInput.current?.click()}>
            Change avatar
          </Button>
          {hasAvatar && (
            <Button variant="ghost" onClick={() => form.setDraft({ avatar: null })}>
              Remove avatar
            </Button>
          )}
          <Button variant="secondary" onClick={() => bannerInput.current?.click()}>
            Change banner
          </Button>
          {hasBanner && (
            <Button variant="ghost" onClick={() => form.setDraft({ banner: null })}>
              Remove banner
            </Button>
          )}
        </div>
        <input
          ref={avatarInput}
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          className="us-sr"
          tabIndex={-1}
          aria-label="Avatar file"
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
          aria-label="Banner file"
          onChange={(e) => {
            void onFile('banner', e.target.files?.[0]);
            e.target.value = '';
          }}
        />
        {error && (
          <Notice variant="danger" title="Couldn't use that image">
            {error}
          </Notice>
        )}
        <p className="us-muted">
          Avatar and banner are saved with the rest of your profile when you choose Save changes.
        </p>
      </div>
      <ImageCropDialog
        open={pick?.kind === 'avatar'}
        file={pick?.kind === 'avatar' ? pick.file : null}
        title="Crop your avatar"
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
        title="Crop your banner"
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
      toast.show({ title: `Updated your nickname in ${guild.name}`, duration: 2500 });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Loaded res={member} what="your membership">
      {() => (
        <form
          className="us-stack"
          onSubmit={(e) => {
            e.preventDefault();
            if (!over) void save();
          }}
        >
          <TextInput
            label="Server nickname"
            value={nick}
            onChange={(e) => setNick(e.target.value)}
            error={over ? `Nicknames can be at most ${NICK_MAX} characters.` : undefined}
            hint="Shown instead of your display name in this server only."
          />
          <div className="us-actions">
            <Button type="submit" loading={busy} disabled={over || nick === current}>
              Save nickname
            </Button>
          </div>
          {error && (
            <Notice variant="danger" title="Couldn't change your nickname">
              {error}
            </Notice>
          )}
        </form>
      )}
    </Loaded>
  );
}

function ServerProfiles({ userId }: { userId: string | undefined }) {
  const guilds = useResource(listGuilds);
  const [picked, setPicked] = useState<string | null>(null);
  return (
    <Card
      id="us-server-profiles"
      title="Per-server profiles"
      description="Choose how you appear in one server."
    >
      <Loaded res={guilds} what="your servers">
        {(rows) => {
          if (rows.length === 0) return <p className="us-muted">You aren&apos;t in any servers.</p>;
          const guild = rows.find((g) => g.id === picked) ?? rows[0];
          return (
            <div className="us-stack">
              <label className="us-muted" id="us-sp-label">
                Server
              </label>
              <Select
                aria-labelledby="us-sp-label"
                options={rows.map((g) => ({ value: g.id, label: g.name }))}
                value={guild?.id ?? null}
                onValueChange={setPicked}
              />
              {guild && userId && <ServerNick key={guild.id} guild={guild} userId={userId} />}
              <Unavailable>
                Only the nickname is stored per server. A separate avatar, banner and about me for
                each server aren&apos;t supported by the server yet.
              </Unavailable>
            </div>
          );
        }}
      </Loaded>
    </Card>
  );
}

export function ProfileSection({ form }: { form: ProfileForm }) {
  const errors = validateProfile(form.draft);
  const bioLen = [...form.draft.bio].length;
  const [tab, setTab] = useState('main');
  return (
    <>
      <SectionTitle>Profiles</SectionTitle>
      {form.loading ? (
        <p className="us-muted" role="status">
          Loading your profile...
        </p>
      ) : form.loadError ? (
        <Notice variant="danger" title="Couldn't load your profile">
          {form.loadError}
        </Notice>
      ) : (
        <>
          <SegmentedControl
            label="Profile"
            value={tab}
            onValueChange={setTab}
            options={[
              { value: 'main', label: 'Main profile' },
              { value: 'servers', label: 'Per-server profiles' },
            ]}
          />
          {tab === 'servers' ? (
            <ServerProfiles userId={form.profile?.user.id} />
          ) : (
            <div className="us-split">
              <div>
                <Card id="us-profile" title="Main profile">
                  <div className="us-stack">
                    <TextInput
                      label="Display name"
                      value={form.draft.displayName}
                      onChange={(e) => form.setDraft({ displayName: e.target.value })}
                      error={errors.displayName}
                      hint="Leave it empty to show your username."
                    />
                    <TextInput
                      label="Pronouns"
                      value={form.draft.pronouns}
                      onChange={(e) => form.setDraft({ pronouns: e.target.value })}
                      error={errors.pronouns}
                    />
                    <div>
                      <TextArea
                        label="About me"
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
                      label="Custom status"
                      value={form.draft.status}
                      onChange={(e) => form.setDraft({ status: e.target.value })}
                      error={errors.status}
                    />
                    <div>
                      <ColorPicker
                        label="Accent color"
                        value={form.draft.accent ?? '#c2b08f'}
                        onValueChange={(hex) => form.setDraft({ accent: hex })}
                      />
                      {form.draft.accent && (
                        <Button
                          variant="link"
                          size="sm"
                          onClick={() => form.setDraft({ accent: null })}
                        >
                          Clear accent color
                        </Button>
                      )}
                    </div>
                  </div>
                </Card>
                <MediaCard form={form} />
                <Card
                  id="us-profile-extras"
                  title="Profile themes, decorations and effects"
                  description="A two-colour profile theme, avatar decorations, profile effects and a server tag."
                >
                  <Unavailable>
                    The server stores one accent colour only. These need profile fields that
                    don&apos;t exist yet, so they&apos;re left out rather than faked.
                  </Unavailable>
                </Card>
                {form.saveError && (
                  <Notice variant="danger" title="Couldn't save your profile">
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
