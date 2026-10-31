import { Button, Notice, TextInput, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { formatDateTime, useT } from '../../../i18n';
import {
  createMemberInvite,
  errorText,
  listMemberInvites,
  revokeMemberInvite,
  useResource,
  type MemberInvite,
  type MemberInvites,
} from '../api';
import { Card, Loaded, SectionTitle } from '../parts';

/**
 * Invites (LNG-USET-025..029, doc 06 section 5): the credit balance, why a code cannot be made right now
 * (generic wording for the suspicion freeze), the create form, open codes with Revoke, and who joined.
 */
export function InvitesSection() {
  const t = useT();
  const toast = useToast();
  const res = useResource(listMemberInvites);
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fresh, setFresh] = useState<MemberInvite | null>(null);

  const link = (i: MemberInvite) => `${window.location.origin}${i.link}`;
  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.show({ title: t('settings.userInvites.copied'), duration: 2000 });
    } catch {
      toast.show({ title: t('settings.userInvites.copyFailed'), duration: 3000 });
    }
  };

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const made = await createMemberInvite(note);
      setFresh(made);
      setNote('');
      res.reload();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const revoke = async (i: MemberInvite) => {
    setError(null);
    try {
      await revokeMemberInvite(i.code);
      if (fresh?.code === i.code) setFresh(null);
      res.reload();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const blockedText = (b: NonNullable<MemberInvites['blocked']>) => {
    switch (b.reason) {
      case 'unverified':
        return t('settings.userInvites.blockedUnverified');
      case 'too_new':
        return t('settings.userInvites.blockedTooNew', { days: b.days ?? 7 });
      case 'at_cap':
        return t('settings.userInvites.blockedAtCap');
      case 'no_credits':
        return t('settings.userInvites.blockedNoCredits');
      case 'at_max':
        return t('settings.userInvites.blockedAtMax');
      default:
        return t('settings.userInvites.blockedFrozen');
    }
  };

  return (
    <>
      <SectionTitle>{t('settings.userInvites.title')}</SectionTitle>
      <Loaded res={res} what={t('settings.userInvites.what')}>
        {(d) => (
          <>
            <Card
              id="us-invite-credits"
              title={t('settings.userInvites.creditsTitle')}
              description={t('settings.userInvites.creditsDesc', { count: d.credits })}
            >
              {d.blocked && (
                <Notice variant="info" role="status">
                  {blockedText(d.blocked)}
                </Notice>
              )}
            </Card>
            <Card
              id="us-invite-create"
              title={t('settings.userInvites.createTitle')}
              description={t('settings.userInvites.createDesc')}
            >
              <TextInput
                label={t('settings.userInvites.noteLabel')}
                hint={t('settings.userInvites.noteHint')}
                value={note}
                maxLength={100}
                onChange={(e) => setNote(e.target.value)}
              />
              <Button loading={busy} disabled={d.blocked !== null} onClick={() => void create()}>
                {t('settings.userInvites.create')}
              </Button>
              {error && (
                <Notice variant="danger" role="alert">
                  {error}
                </Notice>
              )}
              {fresh && (
                <div className="us-field" role="status">
                  <code>{fresh.code}</code>{' '}
                  <Button size="sm" variant="secondary" onClick={() => void copy(fresh.code)}>
                    {t('settings.userInvites.copyCode')}
                  </Button>{' '}
                  <Button size="sm" variant="secondary" onClick={() => void copy(link(fresh))}>
                    {t('settings.userInvites.copyLink')}
                  </Button>
                </div>
              )}
            </Card>
            <Card id="us-invite-open" title={t('settings.userInvites.openTitle')}>
              {d.open.length === 0 ? (
                <p className="us-muted">{t('settings.userInvites.openEmpty')}</p>
              ) : (
                <ul className="us-list">
                  {d.open.map((i) => (
                    <li key={i.code} className="us-row">
                      <div className="us-grow">
                        <code>{i.code}</code>
                        <div className="us-muted">
                          {i.note ? `${i.note} · ` : ''}
                          {t('settings.userInvites.created', {
                            when: formatDateTime(new Date(i.created_at_ms)),
                          })}
                        </div>
                      </div>
                      <Button size="sm" variant="secondary" onClick={() => void copy(link(i))}>
                        {t('settings.userInvites.copyLink')}
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('settings.userInvites.revokeAria', { code: i.code })}
                        onClick={() => void revoke(i)}
                      >
                        {t('settings.userInvites.revoke')}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
            <Card id="us-invite-redeemed" title={t('settings.userInvites.redeemedTitle')}>
              {d.redeemed.length === 0 ? (
                <p className="us-muted">{t('settings.userInvites.redeemedEmpty')}</p>
              ) : (
                <ul className="us-list">
                  {d.redeemed.map((i) => (
                    <li key={i.code} className="us-row">
                      <div className="us-grow">
                        <code>{i.code}</code>
                        <div className="us-muted">
                          {t('settings.userInvites.joined', {
                            username: i.redeemed_by?.username ?? '',
                            when: i.redeemed_at_ms
                              ? formatDateTime(new Date(i.redeemed_at_ms))
                              : '',
                          })}
                        </div>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          </>
        )}
      </Loaded>
    </>
  );
}
