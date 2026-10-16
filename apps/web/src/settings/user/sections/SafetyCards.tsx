// Safety settings over GET/PATCH /users/@me/privacy (D-230): sensitive media per audience (LNG-USET-012),
// the DM spam filter and per-server DM overrides (LNG-USET-013), and the stored age confirmation with its
// re-confirm and reset flow (LNG-USET-015).
import { Button, Notice, SegmentedControl, Select, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import { postAgeGate, setLocalAgeAnswer } from '../../../lounge/AgeGate';
import { useGuilds, useStore } from '../../../store';
import {
  errorText,
  getAgeConfirmation,
  getPrivacy,
  patchPrivacy,
  resetAgeConfirmation,
  useResource,
  type DmSpamFilter,
  type PrivacyPatch,
  type PrivacySettings,
  type SensitiveMediaAudience,
  type SensitiveMediaMode,
} from '../api';
import { Card, fmtDate, Loaded } from '../parts';
import { PasswordPrompt } from './Account';

const DEFAULT_MEDIA: PrivacySettings['sensitive_media'] = {
  friends: 'show',
  others: 'blur',
  servers: 'show',
};

/** Saves a privacy patch with an optimistic update; shared by the cards below. */
export function usePrivacySave() {
  const t = useT();
  const toast = useToast();
  const res = useResource(getPrivacy);
  const [error, setError] = useState<string | null>(null);
  const save = async (patch: PrivacyPatch) => {
    setError(null);
    const before = res.data;
    res.setData((d) => (d ? applyPatch(d, patch) : d));
    try {
      res.setData(await patchPrivacy(patch));
      toast.show({ title: t('settings.privacy.updated'), duration: 1500 });
    } catch (e) {
      if (before) res.setData(before);
      setError(errorText(e));
    }
  };
  return { res, error, save };
}

export function applyPatch(p: PrivacySettings, patch: PrivacyPatch): PrivacySettings {
  const { sensitive_media, server_dm_overrides, ...rest } = patch;
  const overrides = { ...(p.server_dm_overrides ?? {}) };
  for (const [id, v] of Object.entries(server_dm_overrides ?? {})) {
    if (v === null) delete overrides[id];
    else overrides[id] = v;
  }
  return {
    ...p,
    ...rest,
    sensitive_media: { ...DEFAULT_MEDIA, ...p.sensitive_media, ...sensitive_media },
    server_dm_overrides: overrides,
  };
}

const AUDIENCES: { key: SensitiveMediaAudience; label: MessageKey }[] = [
  { key: 'friends', label: 'settings.safety.audFriends' },
  { key: 'others', label: 'settings.safety.audOthers' },
  { key: 'servers', label: 'settings.safety.audServers' },
];

export function SensitiveMediaCard() {
  const t = useT();
  const { res, error, save } = usePrivacySave();
  return (
    <Card
      id="us-filters"
      title={t('settings.privacy.sensitiveTitle')}
      description={t('settings.safety.mediaDesc')}
    >
      <Loaded res={res} what={t('settings.privacy.reachWhat')}>
        {(p) => (
          <div className="us-stack">
            {AUDIENCES.map((a) => (
              <SegmentedControl
                key={a.key}
                label={t(a.label)}
                value={(p.sensitive_media ?? DEFAULT_MEDIA)[a.key]}
                onValueChange={(v) =>
                  void save({ sensitive_media: { [a.key]: v as SensitiveMediaMode } })
                }
                options={[
                  { value: 'show', label: t('settings.safety.modeShow') },
                  { value: 'blur', label: t('settings.safety.modeBlur') },
                  { value: 'block', label: t('settings.safety.modeBlock') },
                ]}
              />
            ))}
            <p className="us-muted">{t('settings.safety.mediaNote')}</p>
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.privacy.saveError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

/** DM spam filter and the per-server DM overrides. */
export function DmSafetyCard() {
  const t = useT();
  const { res, error, save } = usePrivacySave();
  const guilds = useGuilds();
  return (
    <Card
      id="us-dm-filter"
      title={t('settings.safety.dmTitle')}
      description={t('settings.safety.dmDesc')}
    >
      <Loaded res={res} what={t('settings.privacy.reachWhat')}>
        {(p) => (
          <div className="us-stack">
            <SegmentedControl
              label={t('settings.safety.spamFilter')}
              value={p.dm_spam_filter ?? 'non_friends'}
              onValueChange={(v) => void save({ dm_spam_filter: v as DmSpamFilter })}
              options={[
                { value: 'off', label: t('settings.safety.filterOff') },
                { value: 'non_friends', label: t('settings.safety.filterNonFriends') },
                { value: 'all', label: t('settings.safety.filterAll') },
              ]}
            />
            <p className="us-muted">{t('settings.safety.filterHelp')}</p>
            <h3 id="us-dm-overrides-h" className="us-subhead">
              {t('settings.safety.overridesTitle')}
            </h3>
            {guilds.length === 0 ? (
              <p className="us-muted">{t('settings.safety.overridesEmpty')}</p>
            ) : (
              <ul className="us-list" aria-labelledby="us-dm-overrides-h">
                {guilds.map((g) => {
                  const ov = p.server_dm_overrides?.[g.id];
                  const cur = ov === undefined ? 'default' : ov ? 'allow' : 'block';
                  return (
                    <li key={g.id}>
                      <span className="us-row-main" id={`us-dmo-${g.id}`}>
                        {g.name}
                      </span>
                      <Select
                        aria-labelledby={`us-dmo-${g.id}`}
                        value={cur}
                        options={[
                          {
                            value: 'default',
                            label: t(
                              p.allow_server_member_dms
                                ? 'settings.safety.overrideDefaultAllow'
                                : 'settings.safety.overrideDefaultBlock',
                            ),
                          },
                          { value: 'allow', label: t('settings.safety.overrideAllow') },
                          { value: 'block', label: t('settings.safety.overrideBlock') },
                        ]}
                        onValueChange={(v) =>
                          void save({
                            server_dm_overrides: { [g.id]: v === 'default' ? null : v === 'allow' },
                          })
                        }
                      />
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.privacy.saveError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

/** The stored "18 or older" answer: shows when it was given, re-confirms, or resets it behind the password. */
export function AgeConfirmationCard() {
  const t = useT();
  const toast = useToast();
  const res = useResource(getAgeConfirmation);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reset, setReset] = useState(false);
  const userId = useStore((s) => s.currentUserId);
  const reconfirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await postAgeGate(true);
      setLocalAgeAnswer(userId, 'yes');
      res.reload();
      toast.show({ title: t('settings.safety.ageConfirmed'), duration: 2000 });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-age"
      title={t('settings.safety.ageTitle')}
      description={t('settings.safety.ageDesc')}
    >
      <Loaded res={res} what={t('settings.safety.ageWhat')}>
        {(a) => (
          <div className="us-stack">
            <p>
              {a.confirmed_at ? (
                <>
                  <span className="us-badge" data-tone="good">
                    {t('settings.safety.ageYes')}
                  </span>{' '}
                  {t('settings.safety.ageSince', { when: fmtDate(a.confirmed_at) })}
                </>
              ) : (
                <span className="us-badge">{t('settings.safety.ageNo')}</span>
              )}
            </p>
            <div className="us-actions">
              <Button loading={busy} onClick={() => void reconfirm()}>
                {a.confirmed_at
                  ? t('settings.safety.ageReconfirm')
                  : t('settings.safety.ageConfirm')}
              </Button>
              {a.confirmed_at && (
                <Button variant="secondary" onClick={() => setReset(true)}>
                  {t('settings.safety.ageReset')}
                </Button>
              )}
            </div>
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.privacy.saveError')}>
          {error}
        </Notice>
      )}
      <PasswordPrompt
        open={reset}
        danger
        title={t('settings.safety.ageResetTitle')}
        description={t('settings.safety.ageResetDesc')}
        confirmLabel={t('settings.safety.ageResetConfirm')}
        onClose={() => setReset(false)}
        onSubmit={async (pw) => {
          await resetAgeConfirmation(pw);
          setLocalAgeAnswer(userId, null);
          setReset(false);
          res.reload();
          toast.show({ title: t('settings.safety.ageWasReset'), duration: 2500 });
        }}
      />
    </Card>
  );
}
