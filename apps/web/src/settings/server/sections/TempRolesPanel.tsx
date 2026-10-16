// The "Temporary roles" panel of the Features settings (moderation.temp_roles): grant a role for a while and
// end grants early. Loaded lazily through the feature slot registry (featureSlots.tsx).
import { Button, Select } from '@sideroom/ui';
import { useState } from 'react';
import { formatDateTime, useT } from '../../../i18n';
import { useGuild, useMembers } from '../../../store';
import * as api from '../featuresApi';
import { useResource } from '../hooks';
import { Async, useAction } from '../ui';
import type { FeatureSlotProps } from './featureSlots';

/** Grant lengths offered in the temporary-role form, in seconds. */
const DURATIONS = [
  { seconds: 3600, key: 'settings.features.temp.hour' },
  { seconds: 86400, key: 'settings.features.temp.day' },
  { seconds: 7 * 86400, key: 'settings.features.temp.week' },
  { seconds: 30 * 86400, key: 'settings.features.temp.month' },
] as const;

export default function TempRolesPanel({ guildId }: FeatureSlotProps) {
  const t = useT();
  const res = useResource(`temp-roles-${guildId}`, () => api.listTempRoles(guildId));
  const { roles } = useGuild(guildId);
  const { members } = useMembers(guildId);
  const [user, setUser] = useState('');
  const [role, setRole] = useState('');
  const [secs, setSecs] = useState<number>(DURATIONS[1].seconds);
  const act = useAction();

  const userName = (id: string) => {
    const u = members.find((m) => m.member.user_id === id)?.user;
    return u?.global_name || u?.username || id;
  };
  const roleName = (id: string) => roles.find((r) => r.id === id)?.name ?? id;

  const grant = () =>
    void act.run(async () => {
      await api.grantTempRole(guildId, { user_id: user, role_id: role, duration_seconds: secs });
      await res.reload();
    }, t('settings.features.temp.granted'));
  const revoke = (g: api.TempRole) =>
    void act.run(async () => {
      await api.revokeTempRole(guildId, g.user_id, g.role_id);
      await res.reload();
    });

  return (
    <section aria-label={t('settings.features.temp.title')}>
      <h4 className="ss-sub">{t('settings.features.temp.title')}</h4>
      <Async res={res} label={t('settings.features.loading')}>
        {(grants) =>
          grants.length === 0 ? (
            <p className="ss-meta">{t('settings.features.temp.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('settings.features.temp.title')}>
              {grants.map((g) => (
                <li key={`${g.user_id}:${g.role_id}`} className="ss-row">
                  <span>
                    {t('settings.features.temp.line', {
                      user: userName(g.user_id),
                      role: roleName(g.role_id),
                      until: formatDateTime(g.expires_at),
                    })}
                  </span>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={act.busy}
                    aria-label={t('settings.features.temp.revoke', {
                      user: userName(g.user_id),
                      role: roleName(g.role_id),
                    })}
                    onClick={() => revoke(g)}
                  >
                    {t('settings.features.temp.end')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <div className="adm-form">
        <Select
          aria-label={t('settings.features.temp.member')}
          placeholder={t('settings.features.temp.member')}
          searchable
          options={members.map((m) => ({
            value: m.member.user_id,
            label: m.user?.global_name || m.user?.username || m.member.user_id,
          }))}
          value={user}
          onValueChange={setUser}
        />
        <Select
          aria-label={t('settings.features.temp.role')}
          placeholder={t('settings.features.temp.role')}
          options={roles
            .filter((r) => r.id !== guildId)
            .map((r) => ({ value: r.id, label: r.name }))}
          value={role}
          onValueChange={setRole}
        />
        <Select
          aria-label={t('settings.features.temp.length')}
          options={DURATIONS.map((d) => ({ value: String(d.seconds), label: t(d.key) }))}
          value={String(secs)}
          onValueChange={(v) => setSecs(Number(v))}
        />
        <Button disabled={!user || !role || act.busy} onClick={grant}>
          {t('settings.features.temp.give')}
        </Button>
      </div>
    </section>
  );
}
