// LNG-SSET-063: the "Enable Community" wizard. Safety checks, then the rules and updates channels, then the
// finishing touches (default notifications, strip moderation powers from @everyone, agree to the guidelines).
// The two safety requirements (verification level, content filter) are switches the admin turns on themselves and
// Next stays disabled until both are on (G-35, P10-SFE-1); Finish never sets them silently. Everything else is
// sent at Finish: the guild, the @everyone role and the community settings are written in turn.
import { Button, Checkbox, Modal, Notice, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../../i18n';
import { useChannels, useGuild } from '../../../store';
import * as api from '../api';
import { PERM, toBig } from '../permissions';
import { ActionStatus, useAction } from '../ui';

/** Powers a community's @everyone role must not hold (Administrator, roles, channels, server, kick, ban, webhooks). */
export const EVERYONE_MODERATION = [
  'ADMINISTRATOR',
  'MANAGE_ROLES',
  'MANAGE_CHANNELS',
  'MANAGE_GUILD',
  'KICK_MEMBERS',
  'BAN_MEMBERS',
  'MANAGE_WEBHOOKS',
] as const;

const MIN_VERIFICATION = 1;
const FILTER_ALL = 2;

export function CommunityWizard({
  guildId,
  onClose,
  onDone,
}: {
  guildId: string;
  onClose: () => void;
  onDone: (c: api.CommunityObject) => void;
}) {
  const t = useT();
  const { roles } = useGuild(guildId);
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const everyone = roles.find((r) => r.id === guildId);
  const action = useAction();
  const [step, setStep] = useState(0);
  const [rules, setRules] = useState('');
  const [updates, setUpdates] = useState('');
  const [mentionsOnly, setMentionsOnly] = useState(true);
  const [stripPerms, setStripPerms] = useState(true);
  const [agree, setAgree] = useState(false);
  // The store keeps no verification level or content filter, so the live guild object is read for step 1.
  const [current, setCurrent] = useState<api.GuildObject | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api
      .getGuild(guildId)
      .then((g) => live && setCurrent(g))
      .catch((e: unknown) => live && setLoadError(api.errorText(e)));
    return () => {
      live = false;
    };
  }, [guildId]);

  const verificationOk = (current?.verification_level ?? 0) >= MIN_VERIFICATION;
  const filterOk = (current?.explicit_content_filter ?? 0) >= FILTER_ALL;
  // What each safety setting was when the wizard opened: a switch turned on here can be turned back off to that.
  const [original, setOriginal] = useState<{ verification: number; filter: number } | null>(null);
  useEffect(() => {
    if (current && !original)
      setOriginal({
        verification: current.verification_level ?? 0,
        filter: current.explicit_content_filter ?? 0,
      });
  }, [current, original]);
  // A switch that was already on when the wizard opened stays on; only ones turned on here can be undone.
  const verificationLocked = !!original && original.verification >= MIN_VERIFICATION;
  const filterLocked = !!original && original.filter >= FILTER_ALL;

  const setSafety = async (body: api.ModifyGuildBody) => {
    if (!current) return;
    const out = await action.run(() => api.modifyGuild(guildId, body));
    if (out !== undefined && out !== null) setCurrent({ ...current, ...body } as api.GuildObject);
  };

  const strip = (bits: bigint) =>
    EVERYONE_MODERATION.reduce((acc, name) => {
      const bit = PERM[name];
      return bit === undefined ? acc : acc & ~bit;
    }, bits);
  const everyoneBits = toBig(everyone?.permissions);
  const needsStrip = strip(everyoneBits) !== everyoneBits;

  const canNext =
    step === 1 ? !!rules && !!updates : step === 0 ? verificationOk && filterOk : true;
  const lastStep = step === 2;

  const finish = async () => {
    if (!current) return;
    const out = await action.run(async () => {
      // Verification level and content filter are never set here; the admin turned them on in step 1.
      const guildBody: api.ModifyGuildBody = {};
      if (mentionsOnly && current.default_message_notifications !== 1)
        guildBody.default_message_notifications = 1;
      if (Object.keys(guildBody).length > 0) await api.modifyGuild(guildId, guildBody);
      if (stripPerms && needsStrip)
        await api.modifyRole(guildId, guildId, { permissions: strip(everyoneBits).toString() });
      return api.modifyCommunity(guildId, {
        community_enabled: true,
        rules_channel_id: rules,
        public_updates_channel_id: updates,
      });
    });
    if (out) onDone(out);
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('settings.communityWizard.title')}
      description={t('settings.communityWizard.step', { n: step + 1, total: 3 })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.members.cancel')}
          </Button>
          {step > 0 && (
            <Button variant="secondary" onClick={() => setStep(step - 1)}>
              {t('settings.communityWizard.back')}
            </Button>
          )}
          {lastStep ? (
            <Button loading={action.busy} disabled={!agree} onClick={() => void finish()}>
              {t('settings.communityWizard.finish')}
            </Button>
          ) : (
            <Button disabled={!canNext} onClick={() => setStep(step + 1)}>
              {t('settings.communityWizard.next')}
            </Button>
          )}
        </>
      }
    >
      <div className="ss-form">
        <ActionStatus error={action.error ?? loadError} />
        {step === 0 && (
          <>
            <h3>{t('settings.communityWizard.safetyTitle')}</h3>
            <p className="ss-sub">{t('settings.communityWizard.safetyIntro')}</p>
            {current && (
              <>
                <Switch
                  label={t('settings.communityWizard.checkVerification')}
                  description={
                    verificationLocked
                      ? t('settings.communityWizard.alreadyOn')
                      : t('settings.communityWizard.fixVerification')
                  }
                  checked={verificationOk}
                  disabled={verificationLocked || action.busy}
                  onChange={(e) =>
                    void setSafety({
                      verification_level: e.target.checked
                        ? MIN_VERIFICATION
                        : (original?.verification ?? 0),
                    })
                  }
                />
                <Switch
                  label={t('settings.communityWizard.checkFilter')}
                  description={
                    filterLocked
                      ? t('settings.communityWizard.alreadyOn')
                      : t('settings.communityWizard.fixFilter')
                  }
                  checked={filterOk}
                  disabled={filterLocked || action.busy}
                  onChange={(e) =>
                    void setSafety({
                      explicit_content_filter: e.target.checked
                        ? FILTER_ALL
                        : (original?.filter ?? 0),
                    })
                  }
                />
                {!(verificationOk && filterOk) && (
                  <Notice variant="info">{t('settings.communityWizard.safetyNeeded')}</Notice>
                )}
              </>
            )}
          </>
        )}
        {step === 1 && (
          <>
            <h3>{t('settings.communityWizard.basicsTitle')}</h3>
            <WizardChannel
              id="ss-wiz-rules"
              label={t('settings.community.rulesChannel')}
              value={rules}
              onChange={setRules}
              channels={channels}
            />
            <WizardChannel
              id="ss-wiz-updates"
              label={t('settings.community.updatesChannel')}
              value={updates}
              onChange={setUpdates}
              channels={channels}
            />
          </>
        )}
        {step === 2 && (
          <>
            <h3>{t('settings.communityWizard.finishingTitle')}</h3>
            <Checkbox
              label={t('settings.communityWizard.mentionsOnly')}
              description={t('settings.communityWizard.mentionsOnlyHint')}
              checked={mentionsOnly}
              onChange={(e) => setMentionsOnly(e.target.checked)}
            />
            <Checkbox
              label={t('settings.communityWizard.stripPerms')}
              description={
                needsStrip
                  ? t('settings.communityWizard.stripPermsHint')
                  : t('settings.communityWizard.stripPermsNone')
              }
              checked={stripPerms && needsStrip}
              disabled={!needsStrip}
              onChange={(e) => setStripPerms(e.target.checked)}
            />
            <Checkbox
              label={t('settings.communityWizard.agree')}
              checked={agree}
              onChange={(e) => setAgree(e.target.checked)}
            />
            {!agree && <Notice variant="info">{t('settings.communityWizard.agreeNeeded')}</Notice>}
          </>
        )}
      </div>
    </Modal>
  );
}

function WizardChannel({
  id,
  label,
  value,
  onChange,
  channels,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  channels: { id: string; name?: string | null }[];
}) {
  const t = useT();
  return (
    <div>
      <label className="ss-field-label" htmlFor={id}>
        {label}
      </label>
      <select
        id={id}
        className="ss-select"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">{t('settings.communityWizard.chooseChannel')}</option>
        {channels.map((c) => (
          <option key={c.id} value={c.id}>
            #{c.name}
          </option>
        ))}
      </select>
    </div>
  );
}
