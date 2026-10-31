// User Settings > Profile > Who can see my profile (PRF-PRO-001, D-382). A preset picker, then an advanced panel with
// the page's audience and one audience per block (never looser than the page; the clamp is explained), the public
// guestbook toggle, and a live "view as" strip that renders the page through the same server-side filter as a real
// visit. Search engines never index a profile, so there is no toggle for it, only the sentence saying so.
import { Notice, SegmentedControl, Select, Switch } from '@sideroom/ui';
import { useCallback, useEffect, useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import { ProfileRenderer } from '../../../profilepage/ProfileRenderer';
import type { ProfilePage } from '../../../profilepage/types';
import { ApiError } from '../../../api/client';
import { Card } from '../parts';
import {
  BLOCK_FIELDS,
  PRESETS,
  TIERS,
  allowedTiers,
  getPrivacy,
  previewAs,
  putPrivacy,
  type BlockField,
  type Policy,
  type PrivacyUpdate,
  type PrivacyView,
  type Tier,
  type ViewAs,
} from './api';

const TIER_LABEL: Record<Tier, MessageKey> = {
  public: 'profile.privacy.tierPublic',
  members: 'profile.privacy.tierMembers',
  mutual_servers: 'profile.privacy.tierMutualServers',
  friends: 'profile.privacy.tierFriends',
  only_me: 'profile.privacy.tierOnlyMe',
};

const PRESET_LABEL: Record<string, MessageKey> = {
  public: 'profile.privacy.presetPublic',
  members: 'profile.privacy.presetMembers',
  friends: 'profile.privacy.presetFriends',
  private: 'profile.privacy.presetPrivate',
};

const PRESET_HELP: Record<string, MessageKey> = {
  public: 'profile.privacy.presetPublicHelp',
  members: 'profile.privacy.presetMembersHelp',
  friends: 'profile.privacy.presetFriendsHelp',
  private: 'profile.privacy.presetPrivateHelp',
  custom: 'profile.privacy.presetCustomHelp',
};

const FIELD_LABEL: Record<BlockField, MessageKey> = {
  links_tier: 'profile.privacy.fieldLinks',
  bio_tier: 'profile.privacy.fieldBio',
  media_tier: 'profile.privacy.fieldMedia',
  top8_tier: 'profile.privacy.fieldTop8',
  now_playing_tier: 'profile.privacy.fieldNowPlaying',
  guestbook_tier: 'profile.privacy.fieldGuestbook',
  ring_tier: 'profile.privacy.fieldRing',
};

const VIEW_AS: ViewAs[] = ['public', 'member', 'shares_server', 'friend'];
const VIEW_LABEL: Record<ViewAs, MessageKey> = {
  public: 'profile.privacy.viewPublic',
  member: 'profile.privacy.viewMember',
  shares_server: 'profile.privacy.viewShares',
  friend: 'profile.privacy.viewFriend',
};

/** The "view as" strip: the page as a public, member, shared-server or friend viewer gets it. */
export function ViewAsStrip({ refreshKey }: { refreshKey: unknown }) {
  const t = useT();
  const [as, setAs] = useState<ViewAs>('public');
  const [page, setPage] = useState<ProfilePage | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    setPage(undefined);
    previewAs(as).then(
      (p) => live && setPage(p),
      (e: unknown) => {
        // A 404 is the answer: that viewer sees nothing.
        if (live) setPage(e instanceof ApiError && e.status === 404 ? null : undefined);
      },
    );
    return () => {
      live = false;
    };
  }, [as, refreshKey]);
  return (
    <div className="us-view-as">
      <SegmentedControl
        label={t('profile.privacy.viewAs')}
        value={as}
        onValueChange={(v) => setAs(v as ViewAs)}
        options={VIEW_AS.map((v) => ({ value: v, label: t(VIEW_LABEL[v]) }))}
      />
      <div role="region" aria-label={t('profile.privacy.viewRegion')} aria-live="polite">
        {page === undefined ? (
          <p className="us-muted" role="status">
            {t('profile.privacy.viewLoading')}
          </p>
        ) : page === null ? (
          <p className="us-muted">{t('profile.privacy.viewNothing')}</p>
        ) : page.doc.blocks.length === 0 ? (
          <p className="us-muted">{t('profile.privacy.viewNoBlocks')}</p>
        ) : (
          <ProfileRenderer page={page} compact />
        )}
      </div>
    </div>
  );
}

/** A labelled audience select. */
function Field({
  id,
  label,
  hint,
  children,
}: {
  id: string;
  label: string;
  hint?: string;
  children: (labelId: string) => React.ReactNode;
}) {
  const labelId = `${id}-label`;
  return (
    <div className="us-field">
      <span id={labelId} className="us-label">
        {label}
      </span>
      {hint && <p className="us-muted">{hint}</p>}
      {children(labelId)}
    </div>
  );
}

export function PrivacyPanel() {
  const t = useT();
  const [view, setView] = useState<PrivacyView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let live = true;
    getPrivacy().then(
      (v) => live && setView(v),
      () => live && setError(t('profile.privacy.loadError')),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = useCallback(
    async (update: PrivacyUpdate) => {
      setBusy(true);
      setError(null);
      try {
        setView(await putPrivacy(update));
        setVersion((n) => n + 1);
      } catch {
        setError(t('profile.privacy.saveError'));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  const tierOptions = (page: Tier, publicPossible: boolean) =>
    allowedTiers(page).map((tier) => ({
      value: tier,
      label: t(TIER_LABEL[tier]),
      disabled: tier === 'public' && !publicPossible,
    }));

  const stored: Policy | null = view?.stored ?? null;
  return (
    <Card
      id="us-profile-privacy"
      title={t('profile.privacy.title')}
      description={t('profile.privacy.desc')}
    >
      {error && (
        <Notice variant="danger" title={t('profile.privacy.errorTitle')}>
          {error}
        </Notice>
      )}
      {!view || !stored ? (
        !error && (
          <p className="us-muted" role="status">
            {t('profile.privacy.loading')}
          </p>
        )
      ) : (
        <>
          {view.preset === 'private' && (
            // A profile with no choice yet counts as Private (D-382), so members who customised theirs before
            // privacy existed are told why no one else sees it (owner, beta checkpoint 2026-10-30).
            <Notice variant="info" title={t('profile.privacy.privateTitle')}>
              {t('profile.privacy.privateBody')}
            </Notice>
          )}
          <SegmentedControl
            label={t('profile.privacy.preset')}
            value={view.preset === 'custom' ? '' : view.preset}
            onValueChange={(v) => void save({ preset: v as (typeof PRESETS)[number] })}
            options={PRESETS.map((p) => ({ value: p, label: t(PRESET_LABEL[p]!) }))}
          />
          <p className="us-muted">{t(PRESET_HELP[view.preset]!)}</p>
          {!view.public_possible && (
            <Notice variant="info" title={t('profile.privacy.walledTitle')}>
              {t('profile.privacy.walledBody')}
            </Notice>
          )}
          <p className="us-muted">{t('profile.privacy.noIndex')}</p>
          <details className="us-advanced">
            <summary>{t('profile.privacy.advanced')}</summary>
            <div className="us-fields">
              <Field
                id="pv-page"
                label={t('profile.privacy.fieldPage')}
                hint={t('profile.privacy.fieldPageHint')}
              >
                {(labelId) => (
                  <Select
                    aria-labelledby={labelId}
                    disabled={busy}
                    options={TIERS.map((tier) => ({
                      value: tier,
                      label: t(TIER_LABEL[tier]),
                      disabled: tier === 'public' && !view.public_possible,
                    }))}
                    value={stored.page_tier}
                    onValueChange={(v) => void save({ page_tier: v as Tier })}
                  />
                )}
              </Field>
              <p className="us-muted">{t('profile.privacy.clampNote')}</p>
              {BLOCK_FIELDS.map((f) => (
                <Field
                  key={f}
                  id={`pv-${f}`}
                  label={t(FIELD_LABEL[f])}
                  hint={f === 'ring_tier' ? t('profile.privacy.ringHint') : undefined}
                >
                  {(labelId) => (
                    <Select
                      aria-labelledby={labelId}
                      disabled={busy}
                      options={tierOptions(stored.page_tier, view.public_possible)}
                      value={stored[f]}
                      onValueChange={(v) => void save({ [f]: v as Tier })}
                    />
                  )}
                </Field>
              ))}
              <Switch
                label={t('profile.privacy.publicGuestbook')}
                description={t('profile.privacy.publicGuestbookHint')}
                checked={stored.guestbook_accepts_public}
                disabled={
                  busy ||
                  !view.public_possible ||
                  stored.page_tier !== 'public' ||
                  stored.guestbook_tier !== 'public'
                }
                onChange={(e) => void save({ guestbook_accepts_public: e.target.checked })}
              />
            </div>
          </details>
          <h3 className="us-card-subtitle">{t('profile.privacy.previewTitle')}</h3>
          <ViewAsStrip refreshKey={version} />
        </>
      )}
    </Card>
  );
}
