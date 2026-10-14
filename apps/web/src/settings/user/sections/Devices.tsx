import { Button, ConfirmDialog, Notice, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import {
  errorText,
  listSessions,
  listTrustedDevices,
  logoutAll,
  revokeSession,
  revokeTrustedDevice,
  useResource,
} from '../api';
import { Card, fmtDate, Loaded, SectionTitle } from '../parts';

export function TrustedDevicesCard() {
  const t = useT();
  const toast = useToast();
  const res = useResource(listTrustedDevices);
  const [error, setError] = useState<string | null>(null);
  const revoke = async (id: string) => {
    setError(null);
    try {
      await revokeTrustedDevice(id);
      res.setData((d) => d?.filter((x) => x.id !== id));
      toast.show({ title: t('settings.devices.removed'), duration: 2000 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-trusted"
      title={t('settings.devices.trustedTitle')}
      description={t('settings.devices.trustedDesc')}
    >
      <Loaded res={res} what={t('settings.devices.trustedWhat')}>
        {(rows) =>
          rows.length === 0 ? (
            <p className="us-muted">{t('settings.devices.trustedEmpty')}</p>
          ) : (
            <ul className="us-list">
              {rows.map((d) => (
                <li key={d.id}>
                  <div className="us-row-main">
                    <strong>{d.device_name || t('settings.devices.unknownDevice')}</strong>
                    <p className="us-muted">
                      {t('settings.devices.trustedDates', {
                        created: fmtDate(d.created_at),
                        expires: fmtDate(d.expires_at),
                      })}
                    </p>
                  </div>
                  <Button variant="secondary" size="sm" onClick={() => void revoke(d.id)}>
                    {t('settings.devices.revoke')}
                    <span className="us-sr">
                      {' '}
                      {d.device_name || t('settings.devices.deviceFallback')}
                    </span>
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.devices.removeError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

export function SessionsCard({ onSignedOut }: { onSignedOut: () => void }) {
  const t = useT();
  const toast = useToast();
  const res = useResource(listSessions);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [confirmOthers, setConfirmOthers] = useState(false);
  const [busy, setBusy] = useState(false);
  const revoke = async (id: string) => {
    setError(null);
    try {
      await revokeSession(id);
      res.setData((d) => d?.filter((x) => x.id !== id));
      toast.show({ title: t('settings.devices.sessionEnded'), duration: 2000 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  /** There is no "end all others" route; each other session is ended in turn. */
  const others = async () => {
    setBusy(true);
    setError(null);
    try {
      for (const s of (res.data ?? []).filter((x) => !x.current)) {
        await revokeSession(s.id);
        res.setData((d) => d?.filter((x) => x.id !== s.id));
      }
      toast.show({ title: t('settings.devices.othersLoggedOut'), duration: 2500 });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      setConfirmOthers(false);
    }
  };
  const everywhere = async () => {
    setBusy(true);
    try {
      await logoutAll();
      onSignedOut();
    } catch (e) {
      setError(errorText(e));
      setConfirm(false);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-sessions"
      title={t('settings.devices.sessionsTitle')}
      description={t('settings.devices.sessionsDesc')}
      actions={
        <>
          <Button
            variant="secondary"
            disabled={!(res.data ?? []).some((x) => !x.current)}
            onClick={() => setConfirmOthers(true)}
          >
            {t('settings.devices.logOutOthers')}
          </Button>
          <Button variant="danger" onClick={() => setConfirm(true)}>
            {t('settings.devices.logOutEverywhere')}
          </Button>
        </>
      }
    >
      <Loaded res={res} what={t('settings.devices.sessionsWhat')}>
        {(rows) => (
          <ul className="us-list">
            {[...rows]
              .sort((a, b) => Number(b.current) - Number(a.current))
              .map((s) => (
                <li key={s.id}>
                  <div className="us-row-main">
                    <strong>{s.device_name || t('settings.devices.unknownDevice')}</strong>
                    {s.current && (
                      <span className="us-badge">{t('settings.devices.thisDevice')}</span>
                    )}
                    <p className="us-muted">
                      {t('settings.devices.lastActive', { date: fmtDate(s.last_seen_at) })}
                    </p>
                  </div>
                  {!s.current && (
                    <Button variant="secondary" size="sm" onClick={() => void revoke(s.id)}>
                      {t('settings.devices.logOut')}
                      <span className="us-sr">
                        {' '}
                        {s.device_name || t('settings.devices.sessionFallback')}
                      </span>
                    </Button>
                  )}
                </li>
              ))}
          </ul>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.devices.genericError')}>
          {error}
        </Notice>
      )}
      <ConfirmDialog
        open={confirmOthers}
        loading={busy}
        title={t('settings.devices.othersTitle')}
        description={t('settings.devices.othersDesc')}
        confirmLabel={t('settings.devices.logOutOthers')}
        onConfirm={() => void others()}
        onCancel={() => setConfirmOthers(false)}
      />
      <ConfirmDialog
        open={confirm}
        danger
        loading={busy}
        title={t('settings.devices.everywhereTitle')}
        description={t('settings.devices.everywhereDesc')}
        confirmLabel={t('settings.devices.logOutEverywhere')}
        onConfirm={() => void everywhere()}
        onCancel={() => setConfirm(false)}
      />
    </Card>
  );
}

export function DevicesSection({ onSignedOut }: { onSignedOut: () => void }) {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.devices.title')}</SectionTitle>
      <SessionsCard onSignedOut={onSignedOut} />
      <TrustedDevicesCard />
    </>
  );
}
