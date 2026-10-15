import { Button, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { useCurrentUser } from '../../../store';
import * as api from '../api';
import { useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';

/**
 * Two-factor requirement for moderator actions (Discord's `mfa_level`, owner only) and the DM and spam
 * protection switches of Safety Setup (LNG-SSET-052, 054, 055). Each switch saves on its own.
 */
export function ProtectionCards({ guildId }: { guildId: string }) {
  const t = useT();
  const guild = useResource(`guild:${guildId}`, () => api.getGuild(guildId));
  const community = useResource(`community:${guildId}`, () => api.getCommunity(guildId));
  return (
    <>
      <Async res={guild} label={t('settings.protection.loading')}>
        {(g) => <MfaCard guildId={guildId} guild={g} onSaved={(n) => guild.set(n)} />}
      </Async>
      <Async res={community} label={t('settings.protection.loading')}>
        {(c) => <DmCard guildId={guildId} community={c} onSaved={(n) => community.set(n)} />}
      </Async>
    </>
  );
}

function MfaCard({
  guildId,
  guild,
  onSaved,
}: {
  guildId: string;
  guild: api.GuildObject;
  onSaved: (g: api.GuildObject) => void;
}) {
  const t = useT();
  const me = useCurrentUser();
  const action = useAction();
  const isOwner = !!me && guild.owner_id === me.id;
  const on = (guild.mfa_level ?? 0) === 1;
  const toggle = async (next: boolean) => {
    const out = await action.run(
      () => api.setMfaLevel(guildId, next ? 1 : 0),
      t('settings.protection.mfaSaved'),
    );
    if (out) onSaved({ ...guild, mfa_level: out.level });
  };
  return (
    <section className="ss-card" aria-labelledby="ss-mfa">
      <h2 id="ss-mfa">{t('settings.protection.mfaTitle')}</h2>
      <p className="ss-sub">{t('settings.protection.mfaDescription')}</p>
      <ActionStatus error={action.error} done={action.done} />
      <Switch
        label={t('settings.protection.mfaSwitch')}
        checked={on}
        disabled={!isOwner || action.busy}
        onChange={(e) => void toggle(e.target.checked)}
      />
      {!isOwner && <p className="ss-sub">{t('settings.protection.mfaOwnerOnly')}</p>}
    </section>
  );
}

const SWITCHES = [
  ['dm_spam_filter', 'settings.protection.dmSpam'],
  ['explicit_media_filter_dms', 'settings.protection.explicitMedia'],
  ['hide_suspicious_links', 'settings.protection.suspiciousLinks'],
] as const;

function DmCard({
  guildId,
  community,
  onSaved,
}: {
  guildId: string;
  community: api.CommunityObject;
  onSaved: (c: api.CommunityObject) => void;
}) {
  const t = useT();
  const action = useAction();
  const [days, setDays] = useState(String(community.warn_new_account_days ?? 0));
  const save = async (patch: api.CommunityPatch) => {
    const next = await action.run(
      () => api.modifyCommunity(guildId, patch),
      t('settings.protection.saved'),
    );
    if (next) onSaved(next);
  };
  const parsed = Number(days);
  const daysOk = Number.isInteger(parsed) && parsed >= 0 && parsed <= 365;
  return (
    <section className="ss-card" aria-labelledby="ss-dm-protection">
      <h2 id="ss-dm-protection">{t('settings.protection.dmTitle')}</h2>
      <p className="ss-sub">{t('settings.protection.dmDescription')}</p>
      <ActionStatus error={action.error} done={action.done} />
      {SWITCHES.map(([key, label]) => (
        <Switch
          key={key}
          label={t(label)}
          checked={community[key] ?? false}
          disabled={action.busy}
          onChange={(e) => void save({ [key]: e.target.checked })}
        />
      ))}
      <div className="ss-row">
        <TextInput
          label={t('settings.protection.newAccountDays')}
          hint={t('settings.protection.newAccountHint')}
          type="number"
          min={0}
          max={365}
          value={days}
          error={daysOk ? undefined : t('settings.protection.newAccountError')}
          onChange={(e) => setDays(e.target.value)}
        />
        <Button
          size="sm"
          variant="secondary"
          disabled={!daysOk || action.busy || parsed === (community.warn_new_account_days ?? 0)}
          onClick={() => void save({ warn_new_account_days: parsed })}
        >
          {t('settings.protection.save')}
        </Button>
      </div>
    </section>
  );
}
