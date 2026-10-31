import { Badge, Button, Modal, Notice, Radio, RadioGroup, Switch, Tabs } from '@sideroom/ui';
import { useCallback, useEffect, useState } from 'react';
import { useT } from '../../../i18n';
import { errorText } from '../api';
import { PERMISSIONS } from '../permissions';
import { ActionStatus, useAction } from '../ui';
import * as api from './api';
import { Preflight } from './Preflight';

type T = ReturnType<typeof useT>;

/**
 * The whole-server plan (BRG-SYN-004, design 9.5): a dry run of everything the switch would do, reviewed before the
 * owner confirms. Managers prepare and read it; only the owner confirms (the call also needs the MFA step). The
 * confirmation sends the plan's hash back; a 409 means one side changed since, and the plan is rebuilt.
 */
export function PlanView({
  guildId,
  isOwner,
  discordName,
  onClose,
  onStarted,
}: {
  guildId: string;
  isOwner: boolean;
  discordName: string;
  onClose: () => void;
  /** The owner confirmed and the seed started. */
  onStarted: () => Promise<void>;
}) {
  const t = useT();
  const [blocked, setBlocked] = useState(false);
  const [seed, setSeed] = useState<api.SeedStructure>('sr');
  const [planId, setPlanId] = useState<string | null>(null);
  const [plan, setPlan] = useState<api.Plan | null>(null);
  const [stale, setStale] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const act = useAction();

  const build = () =>
    void act.run(async () => {
      setStale(false);
      setPlan(null);
      const { plan_id } = await api.createPlan(guildId, seed);
      setPlanId(plan_id);
    });

  // Look again every 2 s while the plan is being built.
  const building = planId !== null && (plan === null || plan.status === 'building');
  const load = useCallback(async () => {
    if (!planId) return;
    try {
      setPlan(await api.getPlan(guildId, planId));
    } catch (e) {
      act.setError(errorText(e));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId, planId]);
  useEffect(() => {
    if (!building) return;
    void load();
    const timer = setInterval(() => void load(), 2000);
    return () => clearInterval(timer);
  }, [building, load]);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);

  const secondsLeft = plan ? plan.expires_at - Math.floor(now / 1000) : 0;
  const expired = plan !== null && secondsLeft <= 0;
  const hasBlockers = (plan?.blockers.length ?? 0) > 0;

  const edit = (e: api.PlanEdit) =>
    void act.run(async () => {
      if (planId) setPlan(await api.editPlan(guildId, planId, e));
    });

  const confirm = () =>
    void act
      .run(async () => {
        if (!plan) return false;
        try {
          await api.setMode(guildId, {
            mode: 'whole_guild',
            plan_id: plan.plan_id,
            plan_hash: plan.plan_hash,
          });
        } catch (e) {
          if (api.isPlanStale(e)) {
            setStale(true);
            setConfirming(false);
            return false;
          }
          throw e;
        }
        await onStarted();
        return true;
      }, t('settings.bridgeSync.plan.started'))
      .then((ok) => {
        if (ok) setConfirming(false);
      });

  return (
    <section className="ss-card" aria-labelledby="ss-bridge-plan" data-testid="bridge-plan">
      <h2 id="ss-bridge-plan">{t('settings.bridgeSync.plan.title')}</h2>
      <p className="ss-sub">{t('settings.bridgeSync.plan.intro')}</p>
      <Preflight guildId={guildId} onResult={setBlocked} />
      {planId === null && (
        <>
          <RadioGroup
            legend={t('settings.bridgeSync.plan.seedLegend')}
            value={seed}
            onValueChange={(v) => setSeed(v as api.SeedStructure)}
          >
            <Radio value="sr" label={t('settings.bridgeSync.plan.seedSr')} />
            <Radio value="discord" label={t('settings.bridgeSync.plan.seedDiscord')} />
          </RadioGroup>
          <p className="ss-sub">{t('settings.bridgeSync.plan.seedHint')}</p>
          <div className="ss-actions">
            <Button disabled={act.busy || blocked} onClick={build}>
              {t('settings.bridgeSync.plan.build')}
            </Button>
            <Button variant="secondary" onClick={onClose}>
              {t('settings.bridgeSync.plan.close')}
            </Button>
          </div>
        </>
      )}
      {planId !== null && !plan && (
        <p role="status" className="ss-sub">
          {t('settings.bridgeSync.plan.building')}
        </p>
      )}
      {plan?.status === 'building' && (
        <p role="status" className="ss-sub">
          {t('settings.bridgeSync.plan.building')}
        </p>
      )}
      {plan?.status === 'failed' && (
        <Notice variant="danger">{t('settings.bridgeSync.plan.failed')}</Notice>
      )}
      {plan?.status === 'ready' && (
        <>
          {hasBlockers && (
            <Notice variant="danger" title={t('settings.bridgeSync.plan.blockersTitle')}>
              <ul>
                {plan.blockers.map((b) => (
                  <li key={b.code}>
                    {blockerText(b.code, t)}
                    {b.detail ? ` ${b.detail}` : ''}
                  </li>
                ))}
              </ul>
            </Notice>
          )}
          {stale && <Notice variant="warning">{t('settings.bridgeSync.plan.stale')}</Notice>}
          <p className="ss-sub">
            {t('settings.bridgeSync.plan.estimate', {
              calls: plan.estimate.discord_calls,
              minutes: Math.max(1, Math.ceil(plan.estimate.seconds / 60)),
            })}{' '}
            {expired
              ? t('settings.bridgeSync.plan.expired')
              : t('settings.bridgeSync.plan.expiresIn', {
                  minutes: Math.max(1, Math.ceil(secondsLeft / 60)),
                })}
          </p>
          <PlanTabs plan={plan} t={t} busy={act.busy} edit={edit} />
          <div className="ss-actions">
            <Button
              disabled={!isOwner || hasBlockers || expired || stale || act.busy}
              onClick={() => setConfirming(true)}
            >
              {t('settings.bridgeSync.plan.confirm')}
            </Button>
            <Button variant="secondary" disabled={act.busy} onClick={build}>
              {t('settings.bridgeSync.plan.rebuild')}
            </Button>
            <Button variant="secondary" onClick={onClose}>
              {t('settings.bridgeSync.plan.close')}
            </Button>
          </div>
          {!isOwner && <p className="ss-sub">{t('settings.bridgeSync.plan.ownerOnly')}</p>}
        </>
      )}
      <ActionStatus error={act.error} done={act.done} />
      {confirming && (
        <Modal
          open
          size="sm"
          onClose={() => setConfirming(false)}
          title={t('settings.bridgeSync.plan.confirmTitle')}
          footer={
            <>
              <Button variant="secondary" disabled={act.busy} onClick={() => setConfirming(false)}>
                {t('settings.bridgeSync.mode.cancel')}
              </Button>
              <Button disabled={act.busy} onClick={confirm}>
                {t('settings.bridgeSync.plan.confirmStart')}
              </Button>
            </>
          }
        >
          <p>{t('settings.bridgeSync.plan.confirmBody')}</p>
          <p>
            {t('settings.bridgeSync.privacy.body', {
              name: discordName || t('settings.bridgeSync.privacy.noName'),
            })}
          </p>
          {act.error && <Notice variant="danger">{act.error}</Notice>}
        </Modal>
      )}
    </section>
  );
}

function blockerText(code: api.PlanBlockerCode, t: T): string {
  switch (code) {
    case 'bot_admin':
      return t('settings.bridgeSync.plan.blockBotAdmin');
    case 'bot_top_role':
      return t('settings.bridgeSync.plan.blockBotTopRole');
    case 'intents':
      return t('settings.bridgeSync.plan.blockIntents');
    case 'caps':
      return t('settings.bridgeSync.plan.blockCaps');
    case 'unique_guild':
      return t('settings.bridgeSync.plan.blockUniqueGuild');
    case 'staff_guild':
      return t('settings.bridgeSync.plan.blockStaffGuild');
  }
}

function actionText(a: api.PlanAction, t: T): string {
  switch (a) {
    case 'match':
      return t('settings.bridgeSync.plan.actionMatch');
    case 'pinned':
      return t('settings.bridgeSync.plan.actionPinned');
    case 'create_here':
      return t('settings.bridgeSync.plan.actionCreateHere');
    case 'create_there':
      return t('settings.bridgeSync.plan.actionCreateThere');
  }
}

function skipText(r: api.NotMirroredReason, t: T): string {
  switch (r) {
    case 'e2ee':
      return t('settings.bridgeSync.plan.skipE2ee');
    case 'dm':
      return t('settings.bridgeSync.plan.skipDm');
    case 'sr_only_bit':
      return t('settings.bridgeSync.plan.skipSrOnlyBit');
    case 'member_overwrite':
      return t('settings.bridgeSync.plan.skipMemberOverwrite');
    case 'managed_role':
      return t('settings.bridgeSync.plan.skipManagedRole');
    case 'degraded_type':
      return t('settings.bridgeSync.plan.skipDegradedType');
    case 'cap':
      return t('settings.bridgeSync.plan.skipCap');
    case 'voice_media':
      return t('settings.bridgeSync.plan.skipVoiceMedia');
  }
}

const permLabel = (id: string) => PERMISSIONS.find((p) => p.id === id)?.label ?? id;

function PlanTabs({
  plan,
  t,
  busy,
  edit,
}: {
  plan: api.Plan;
  t: T;
  busy: boolean;
  edit: (e: api.PlanEdit) => void;
}) {
  const channelRows = [...plan.categories, ...plan.channels];
  const tabs = [
    {
      id: 'roles',
      label: t('settings.bridgeSync.plan.tabRoles', { n: plan.roles.length }),
      content: <RowsTable rows={plan.roles} t={t} busy={busy} edit={edit} />,
    },
    {
      id: 'channels',
      label: t('settings.bridgeSync.plan.tabChannels', { n: channelRows.length }),
      content: <RowsTable rows={channelRows} t={t} busy={busy} edit={edit} />,
    },
    {
      id: 'threads',
      label: t('settings.bridgeSync.plan.tabThreads', { n: plan.threads.length }),
      content: <RowsTable rows={plan.threads} t={t} busy={busy} edit={edit} />,
    },
    {
      id: 'changes',
      label: t('settings.bridgeSync.plan.tabChanges', { n: plan.discord_changes.length }),
      content:
        plan.discord_changes.length === 0 ? (
          <p className="ss-na">{t('settings.bridgeSync.plan.empty')}</p>
        ) : (
          <ul>
            {plan.discord_changes.map((c) => (
              <li key={c.id}>{c.text}</li>
            ))}
          </ul>
        ),
    },
    {
      id: 'imports',
      label: t('settings.bridgeSync.plan.tabImports', { n: plan.imports.length }),
      content: (
        <>
          <p className="ss-sub">{t('settings.bridgeSync.plan.importsIntro')}</p>
          {plan.imports.length === 0 && (
            <p className="ss-na">{t('settings.bridgeSync.plan.empty')}</p>
          )}
          <ul className="ss-bridge-imports">
            {plan.imports.map((i) => (
              <li key={i.id}>
                <Switch
                  label={t('settings.bridgeSync.plan.importToggle', { name: i.name })}
                  checked={i.import}
                  disabled={busy}
                  onChange={(e) => edit({ op: 'import', row_id: i.id, import: e.target.checked })}
                />
                <p className="ss-sub">
                  {i.permissions.length === 0
                    ? t('settings.bridgeSync.plan.noPermissions')
                    : i.permissions
                        .filter((p) => !i.dangerous.includes(p))
                        .map(permLabel)
                        .join(', ')}
                </p>
                {i.dangerous.length > 0 && (
                  <p className="ss-bridge-danger">
                    {t('settings.bridgeSync.plan.dangerous', {
                      list: i.dangerous.map(permLabel).join(', '),
                    })}
                  </p>
                )}
              </li>
            ))}
          </ul>
        </>
      ),
    },
    {
      id: 'skipped',
      label: t('settings.bridgeSync.plan.tabSkipped', { n: plan.not_mirrored.length }),
      content:
        plan.not_mirrored.length === 0 ? (
          <p className="ss-na">{t('settings.bridgeSync.plan.empty')}</p>
        ) : (
          <ul>
            {plan.not_mirrored.map((n) => (
              <li key={n.id}>
                {n.name} <Badge>{skipText(n.reason, t)}</Badge>
              </li>
            ))}
          </ul>
        ),
    },
    {
      id: 'blockers',
      label: t('settings.bridgeSync.plan.tabBlockers', { n: plan.blockers.length }),
      content:
        plan.blockers.length === 0 ? (
          <p className="ss-na">{t('settings.bridgeSync.plan.empty')}</p>
        ) : (
          <ul>
            {plan.blockers.map((b) => (
              <li key={b.code}>{blockerText(b.code, t)}</li>
            ))}
          </ul>
        ),
    },
  ];
  return <Tabs items={tabs} label={t('settings.bridgeSync.plan.tabsLabel')} className="ss-tabs" />;
}

function RowsTable({
  rows,
  t,
  busy,
  edit,
}: {
  rows: api.PlanRow[];
  t: T;
  busy: boolean;
  edit: (e: api.PlanEdit) => void;
}) {
  const [here, setHere] = useState('');
  const [there, setThere] = useState('');
  if (rows.length === 0) return <p className="ss-na">{t('settings.bridgeSync.plan.empty')}</p>;
  const onlyHere = rows.filter((r) => r.action === 'create_there');
  const onlyThere = rows.filter((r) => r.action === 'create_here');
  return (
    <>
      <table className="ss-table">
        <thead>
          <tr>
            <th scope="col">{t('settings.bridgeSync.plan.colName')}</th>
            <th scope="col">{t('settings.bridgeSync.plan.colDiscord')}</th>
            <th scope="col">{t('settings.bridgeSync.plan.colWhat')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.id}>
              <td>{r.action === 'create_here' ? '' : r.name}</td>
              <td>{r.action === 'create_here' ? r.name : (r.discord_name ?? '')}</td>
              <td>
                {actionText(r.action, t)}
                {r.note ? <span className="ss-sub"> ({r.note})</span> : null}{' '}
                {r.action === 'match' && (
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy}
                    aria-label={t('settings.bridgeSync.plan.unlink', { name: r.name })}
                    onClick={() => edit({ op: 'unlink', row_id: r.id })}
                  >
                    {t('settings.bridgeSync.plan.unlinkShort')}
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {onlyHere.length > 0 && onlyThere.length > 0 && (
        <fieldset className="ss-bridge-add">
          <legend>{t('settings.bridgeSync.plan.linkTitle')}</legend>
          <label className="ss-field-label">
            {t('settings.bridgeSync.plan.linkHere')}
            <select className="ss-select" value={here} onChange={(e) => setHere(e.target.value)}>
              <option value="">{t('settings.bridgeSync.plan.linkChoose')}</option>
              {onlyHere.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                </option>
              ))}
            </select>
          </label>
          <label className="ss-field-label">
            {t('settings.bridgeSync.plan.linkThere')}
            <select className="ss-select" value={there} onChange={(e) => setThere(e.target.value)}>
              <option value="">{t('settings.bridgeSync.plan.linkChoose')}</option>
              {onlyThere.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                </option>
              ))}
            </select>
          </label>
          <div className="ss-actions">
            <Button
              size="sm"
              disabled={busy || !here || !there}
              onClick={() => {
                const h = onlyHere.find((r) => r.id === here);
                const d = onlyThere.find((r) => r.id === there);
                if (h?.sr_id && d?.discord_id)
                  edit({ op: 'link', kind: h.kind, sr_id: h.sr_id, discord_id: d.discord_id });
                setHere('');
                setThere('');
              }}
            >
              {t('settings.bridgeSync.plan.link')}
            </Button>
          </div>
        </fieldset>
      )}
    </>
  );
}
