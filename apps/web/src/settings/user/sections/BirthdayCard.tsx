// User Settings > Profiles > Servers > Birthday (FTR-BDAY-001): month and day (the year only if wanted) and who may
// see it. "Hidden" keeps it private: no server announces it and it is not listed. The channel and role a server uses
// are set by that server's staff under Server Settings > Features.
import { Button, Checkbox, Notice, Select, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../../api/client';
import { formatDate, useT } from '../../../i18n';
import {
  clearBirthday,
  getBirthday,
  setBirthday,
  type Birthday,
} from '../../../lounge/features/api';
import { errorText } from '../api';
import { Card } from '../parts';

const MONTHS = Array.from({ length: 12 }, (_, i) => i + 1);

/** Days in a month for a leap-friendly year (29 February is allowed; the server celebrates it on the 28th). */
export const daysIn = (month: number): number => new Date(Date.UTC(2000, month, 0)).getUTCDate();

export function BirthdayCard() {
  const t = useT();
  const [loaded, setLoaded] = useState(false);
  const [saved, setSaved] = useState<Birthday | null>(null);
  const [month, setMonth] = useState(1);
  const [day, setDay] = useState(1);
  const [year, setYear] = useState('');
  const [share, setShare] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<'saved' | 'cleared' | null>(null);

  const apply = (b: Birthday | null) => {
    setSaved(b);
    if (b) {
      setMonth(b.month);
      setDay(b.day);
      setYear(b.year ? String(b.year) : '');
      setShare(b.visibility !== 'hidden');
    }
  };

  useEffect(() => {
    let live = true;
    getBirthday()
      .then((b) => live && apply(b))
      .catch((e: unknown) => {
        // 404 means none set yet.
        if (live && !(e instanceof ApiError && e.status === 404))
          setError(t('settings.birthday.loadError'));
      })
      .finally(() => live && setLoaded(true));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const yearNum = year.trim() === '' ? null : Number(year);
  const yearBad =
    yearNum !== null && !(Number.isInteger(yearNum) && yearNum >= 1900 && yearNum <= 2100);
  const dayMax = daysIn(month);

  const save = async () => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const b = await setBirthday({
        month,
        day: Math.min(day, dayMax),
        year: yearNum,
        visibility: share ? 'servers' : 'hidden',
      });
      apply(b);
      setDone('saved');
    } catch (e) {
      setError(errorText(e, t('settings.birthday.saveError')));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      await clearBirthday();
      setSaved(null);
      setYear('');
      setDone('cleared');
    } catch (e) {
      setError(errorText(e, t('settings.birthday.saveError')));
    } finally {
      setBusy(false);
    }
  };

  const monthName = (m: number) =>
    formatDate(new Date(Date.UTC(2000, m - 1, 1)), { month: 'long', timeZone: 'UTC' });

  return (
    <Card
      id="us-birthday"
      title={t('settings.birthday.title')}
      description={t('settings.birthday.description')}
    >
      {!loaded ? (
        <p className="us-muted" role="status">
          {t('settings.birthday.loading')}
        </p>
      ) : (
        <div className="us-stack">
          <Select
            aria-label={t('settings.birthday.month')}
            options={MONTHS.map((m) => ({ value: String(m), label: monthName(m) }))}
            value={String(month)}
            onValueChange={(v) => {
              setMonth(Number(v));
              setDay((d) => Math.min(d, daysIn(Number(v))));
            }}
          />
          <Select
            aria-label={t('settings.birthday.day')}
            options={Array.from({ length: dayMax }, (_, i) => ({
              value: String(i + 1),
              label: String(i + 1),
            }))}
            value={String(Math.min(day, dayMax))}
            onValueChange={(v) => setDay(Number(v))}
          />
          <TextInput
            label={t('settings.birthday.year')}
            hint={t('settings.birthday.yearHint')}
            inputMode="numeric"
            value={year}
            maxLength={4}
            error={yearBad ? t('settings.birthday.yearInvalid') : undefined}
            onChange={(e) => setYear(e.target.value.replace(/\D/g, ''))}
          />
          <Checkbox
            label={t('settings.birthday.share')}
            description={t('settings.birthday.shareHint')}
            checked={share}
            onChange={(e) => setShare(e.target.checked)}
          />
          <div className="us-actions">
            <Button disabled={busy || yearBad} onClick={() => void save()}>
              {t('settings.birthday.save')}
            </Button>
            {saved && (
              <Button variant="secondary" disabled={busy} onClick={() => void remove()}>
                {t('settings.birthday.remove')}
              </Button>
            )}
          </div>
          {done && (
            <p className="us-muted" role="status">
              {done === 'saved' ? t('settings.birthday.saved') : t('settings.birthday.cleared')}
            </p>
          )}
          {error && (
            <Notice variant="danger" role="alert">
              {error}
            </Notice>
          )}
        </div>
      )}
    </Card>
  );
}
