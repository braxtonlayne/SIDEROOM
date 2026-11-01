// "Your account review" and "Moderation transparency" (MOD-GRD-001, D-395, D-423, D-424). The first lists what is in
// force on the member's own account in plain words (never a case, a staff member or the invite tree) and lets them
// appeal each restriction once; a frozen account can sign in and do this. The second is the monthly counts page:
// category and counts only.
import { Button, TextArea } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { formatDate, useT, type MessageKey } from '../../../i18n';
import * as g from '../../../moderation/graduated';
import { Card } from '../parts';

const APPEAL_KEYS: Record<NonNullable<g.MyRestriction['appeal_state']>, MessageKey> = {
  open: 'settings.privacy.review.appealOpen',
  upheld: 'settings.privacy.review.appealUpheld',
  overturned: 'settings.privacy.review.appealOverturned',
};

export function AccountReviewCard() {
  const t = useT();
  const [items, setItems] = useState<g.MyRestriction[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [message, setMessage] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState(false);
  const load = () =>
    g
      .myRestrictions()
      .then(setItems)
      .catch(() => setItems([]));
  useEffect(() => {
    let live = true;
    g.myRestrictions()
      .then((r) => live && setItems(r))
      .catch(() => live && setItems([]));
    return () => {
      live = false;
    };
  }, []);
  // Nothing in force: the card stays out of the way.
  if (!items || items.length === 0) return null;
  const send = async (id: string) => {
    setError(null);
    try {
      await g.appealRestriction(id, message.trim());
      setSent(true);
      setOpen(null);
      setMessage('');
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  return (
    <Card
      id="account-review"
      title={t('settings.privacy.review.title')}
      description={t('settings.privacy.review.description')}
    >
      {sent && (
        <p role="status" className="us-muted">
          {t('settings.privacy.review.sent')}
        </p>
      )}
      <ul className="us-list" aria-label={t('settings.privacy.review.listLabel')}>
        {items.map((r) => (
          <li key={r.id}>
            <div className="us-row-main">
              <strong>{r.effect}</strong>
              <span className="us-muted">
                {' '}
                {r.reason} · {t('settings.privacy.review.until', { when: formatDate(r.ends_at_ms) })}
                {r.appeal_state ? ` · ${t(APPEAL_KEYS[r.appeal_state])}` : ''}
              </span>
            </div>
            {r.can_appeal && open !== r.id && (
              <Button size="sm" variant="secondary" onClick={() => setOpen(r.id)}>
                {t('settings.privacy.review.appeal')}
              </Button>
            )}
            {open === r.id && (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void send(r.id);
                }}
              >
                <TextArea
                  label={t('settings.privacy.review.appealLabel')}
                  value={message}
                  maxLength={2000}
                  rows={3}
                  onChange={(e) => setMessage(e.target.value)}
                />
                {error && <p role="alert">{error}</p>}
                <Button type="submit" disabled={message.trim().length < 2}>
                  {t('settings.privacy.review.send')}
                </Button>
              </form>
            )}
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function TransparencyCard() {
  const t = useT();
  const [months, setMonths] = useState<g.TransparencyMonth[] | null>(null);
  useEffect(() => {
    let live = true;
    g.transparency()
      .then((r) => live && setMonths(r.months))
      .catch(() => live && setMonths([]));
    return () => {
      live = false;
    };
  }, []);
  if (!months) return null;
  return (
    <Card
      id="moderation-transparency"
      title={t('settings.privacy.transparency.title')}
      description={t('settings.privacy.transparency.description')}
    >
      {months.length === 0 ? (
        <p className="us-muted">{t('settings.privacy.transparency.empty')}</p>
      ) : (
        <ul className="us-list" aria-label={t('settings.privacy.transparency.listLabel')}>
          {months.map((m) => (
            <li key={m.month}>
              <div className="us-row-main">
                <strong>{m.month}</strong>
                <span className="us-muted">
                  {' '}
                  {t('settings.privacy.transparency.month', {
                    serious: m.serious_bans,
                    branch: m.branch_actions,
                    actioned: m.accounts_actioned,
                    frozen: m.accounts_frozen,
                    sterilizations: m.sterilizations,
                    appeals: m.appeals_filed,
                    overturned: m.appeals_overturned,
                  })}
                  {m.by_category.length > 0 &&
                    ` · ${m.by_category.map((c) => `${c.category}: ${c.notices}`).join(', ')}`}
                </span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
