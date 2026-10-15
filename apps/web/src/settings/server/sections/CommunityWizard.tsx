// LNG-SSET-063: the "Enable Community" wizard. Safety checks, then the rules and updates channels, then the
// finishing touches (default notifications, strip moderation powers from @everyone, agree to the guidelines).
// Nothing is sent until Finish; then the guild, the @everyone role and the community settings are written in turn.
import { Button, Checkbox, Modal, Notice } from '@sideroom/ui';
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

  const strip = (bits: bigint) =>
    EVERYONE_MODERATION.reduce((acc, name) => {
      const bit = PERM[name];
      return bit === undefined ? acc : acc & ~bit;
    }, bits);
  const everyoneBits = toBig(everyone?.permissions);
  const needsStrip = strip(everyoneBits) !== everyoneBits;

  const canNext = step === 1 ? !!rules && !!updates : step === 0 ? !!current : true;
  const lastStep = step === 2;

  const finish = async () => {
    if (!current) return;
    const out = await action.run(async () => {
      const guildBody: api.ModifyGuildBody = {};
      if (!verificationOk) guildBody.verification_level = MIN_VERIFICATION;
      if (!filterOk) guildBody.explicit_content_filter = FILTER_ALL;
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

  const check = (ok: boolean, label: string, fix: string) => (
    <li className="ss-row">
      <span className="ss-grow">
        <strong>{label}</strong>
        {!ok && <span className="ss-sub"> {fix}</span>}
      </span>
      <span>
        {ok ? t('settings.communityWizard.passed') : t('settings.communityWizard.willFix')}
      </span>
    </li>
  );

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
              <ul className="ss-list" aria-label={t('settings.communityWizard.safetyTitle')}>
                {check(
                  verificationOk,
                  t('settings.communityWizard.checkVerification'),
                  t('settings.communityWizard.fixVerification'),
                )}
                {check(
                  filterOk,
                  t('settings.communityWizard.checkFilter'),
                  t('settings.communityWizard.fixFilter'),
                )}
              </ul>
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
