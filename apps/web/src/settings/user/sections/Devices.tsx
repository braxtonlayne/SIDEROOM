import { Button, ConfirmDialog, Notice, useToast } from '@sideroom/ui';
import { useState } from 'react';
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
  const toast = useToast();
  const res = useResource(listTrustedDevices);
  const [error, setError] = useState<string | null>(null);
  const revoke = async (id: string) => {
    setError(null);
    try {
      await revokeTrustedDevice(id);
      res.setData((d) => d?.filter((x) => x.id !== id));
      toast.show({ title: 'Device removed', duration: 2000 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-trusted"
      title="Trusted devices"
      description="Devices that skip the two-factor prompt."
    >
      <Loaded res={res} what="trusted devices">
        {(rows) =>
          rows.length === 0 ? (
            <p className="us-muted">No trusted devices.</p>
          ) : (
            <ul className="us-list">
              {rows.map((d) => (
                <li key={d.id}>
                  <div className="us-row-main">
                    <strong>{d.device_name || 'Unknown device'}</strong>
                    <p className="us-muted">
                      Added {fmtDate(d.created_at)}, expires {fmtDate(d.expires_at)}
                    </p>
                  </div>
                  <Button variant="secondary" size="sm" onClick={() => void revoke(d.id)}>
                    Revoke<span className="us-sr"> {d.device_name || 'device'}</span>
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
      {error && (
        <Notice variant="danger" title="Couldn't remove the device">
          {error}
        </Notice>
      )}
    </Card>
  );
}

export function SessionsCard({ onSignedOut }: { onSignedOut: () => void }) {
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
      toast.show({ title: 'Session ended', duration: 2000 });
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
      toast.show({ title: 'Logged out of your other devices', duration: 2500 });
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
      title="Devices and sessions"
      description="Where you're signed in."
      actions={
        <>
          <Button
            variant="secondary"
            disabled={!(res.data ?? []).some((x) => !x.current)}
            onClick={() => setConfirmOthers(true)}
          >
            Log out other devices
          </Button>
          <Button variant="danger" onClick={() => setConfirm(true)}>
            Log out everywhere
          </Button>
        </>
      }
    >
      <Loaded res={res} what="sessions">
        {(rows) => (
          <ul className="us-list">
            {[...rows]
              .sort((a, b) => Number(b.current) - Number(a.current))
              .map((s) => (
                <li key={s.id}>
                  <div className="us-row-main">
                    <strong>{s.device_name || 'Unknown device'}</strong>
                    {s.current && <span className="us-badge">This device</span>}
                    <p className="us-muted">Last active {fmtDate(s.last_seen_at)}</p>
                  </div>
                  {!s.current && (
                    <Button variant="secondary" size="sm" onClick={() => void revoke(s.id)}>
                      Log out<span className="us-sr"> {s.device_name || 'session'}</span>
                    </Button>
                  )}
                </li>
              ))}
          </ul>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title="Something went wrong">
          {error}
        </Notice>
      )}
      <ConfirmDialog
        open={confirmOthers}
        loading={busy}
        title="Log out your other devices?"
        description="Every other browser and app is signed out. This device stays signed in."
        confirmLabel="Log out other devices"
        onConfirm={() => void others()}
        onCancel={() => setConfirmOthers(false)}
      />
      <ConfirmDialog
        open={confirm}
        danger
        loading={busy}
        title="Log out everywhere?"
        description="This ends every session, including this one. You will need to sign in again."
        confirmLabel="Log out everywhere"
        onConfirm={() => void everywhere()}
        onCancel={() => setConfirm(false)}
      />
    </Card>
  );
}

export function DevicesSection({ onSignedOut }: { onSignedOut: () => void }) {
  return (
    <>
      <SectionTitle>Devices</SectionTitle>
      <SessionsCard onSignedOut={onSignedOut} />
      <TrustedDevicesCard />
    </>
  );
}
