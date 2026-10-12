import { Avatar, Button, ColorPicker, Notice, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  errorText,
  getOwnProfile,
  getPresence,
  patchSelf,
  type OwnProfile,
  type ProfilePatch,
} from '../api';
import { Card, SectionTitle, Unavailable } from '../parts';

export const BIO_MAX = 190;
export const PRONOUNS_MAX = 40;
export const STATUS_MAX = 128;

export interface ProfileDraft {
  pronouns: string;
  bio: string;
  /** `#rrggbb` or null for none. */
  accent: string | null;
  status: string;
}

const EMPTY: ProfileDraft = { pronouns: '', bio: '', accent: null, status: '' };

const toHex = (n: number | null | undefined) =>
  typeof n === 'number' ? `#${n.toString(16).padStart(6, '0')}` : null;
const fromHex = (h: string) => parseInt(h.slice(1), 16);

export function validateProfile(d: ProfileDraft): Partial<Record<keyof ProfileDraft, string>> {
  const errors: Partial<Record<keyof ProfileDraft, string>> = {};
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
  if (draft.bio !== saved.bio) patch.bio = draft.bio.trim() === '' ? null : draft.bio;
  if (draft.pronouns !== saved.pronouns)
    patch.pronouns = draft.pronouns.trim() === '' ? null : draft.pronouns;
  if (draft.accent !== saved.accent)
    patch.accent_color = draft.accent ? fromHex(draft.accent) : null;
  if (draft.status !== saved.status)
    patch.custom_status = draft.status.trim() === '' ? null : { text: draft.status };
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
      await patchSelf(patch);
      setSaved(draft);
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
  const name = u?.global_name || u?.username || 'You';
  const accent = form.draft.accent;
  return (
    <aside className="us-preview" aria-label="Profile preview">
      <div className="us-preview-banner" style={accent ? { background: accent } : undefined} />
      <div className="us-preview-body">
        <Avatar name={name} size={80} className="us-preview-avatar" />
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

export function ProfileSection({ form }: { form: ProfileForm }) {
  const errors = validateProfile(form.draft);
  const bioLen = [...form.draft.bio].length;
  return (
    <>
      <SectionTitle>Profile</SectionTitle>
      {form.loading ? (
        <p className="us-muted" role="status">
          Loading your profile...
        </p>
      ) : form.loadError ? (
        <Notice variant="danger" title="Couldn't load your profile">
          {form.loadError}
        </Notice>
      ) : (
        <div className="us-split">
          <div>
            <Card id="us-profile" title="Main profile">
              <div className="us-stack">
                <TextInput
                  label="Display name"
                  value={form.profile?.user.global_name ?? ''}
                  readOnly
                  disabled
                  hint="Changing your display name isn't supported by the server yet."
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
            <Card id="us-media" title="Avatar and banner">
              <div className="us-actions">
                <Button variant="secondary" disabled>
                  Change avatar
                </Button>
                <Button variant="secondary" disabled>
                  Change banner
                </Button>
              </div>
              <Unavailable>Avatar and banner uploads are coming soon.</Unavailable>
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
  );
}
