import { Button, TextInput, useToast } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { useT } from '../../i18n';
import { errorText, inviteTester, isUnavailable, keys, listTesters, removeTester } from '../api';
import type { AppTester, DevApp } from '../types';
import { LoadError, Loading } from '../ui';

/** Testers an application may have, invited and accepted together (D-204). */
export const MAX_TESTERS = 25;

/**
 * Testers of a development application (G-01, D-204). The owner invites people by username; each gets a system
 * DM with Accept and Decline. Accepted testers can add the application to servers with at most 25 members where
 * they can manage the server, like the owner. Hidden once the application is released.
 */
export function TestersCard({ app }: { app: DevApp }) {
  const t = useT();
  const toast = useToast();
  const qc = useQueryClient();
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const q = useQuery({ queryKey: keys.testers(app.id), queryFn: () => listTesters(app.id) });
  const refresh = () => void qc.invalidateQueries({ queryKey: keys.testers(app.id) });

  const invite = useMutation({
    mutationFn: (username: string) => inviteTester(app.id, username),
    onSuccess: (tester) => {
      setName('');
      setError(null);
      refresh();
      toast.show({
        title: t('developers.testers.invited', { name: tester.username ?? name.trim() }),
        variant: 'success',
      });
    },
    onError: (e) => setError(errorText(e, t('developers.testers.inviteFailed'))),
  });
  const remove = useMutation({
    mutationFn: (tester: AppTester) => removeTester(app.id, tester.user_id),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e, t('developers.testers.removeFailed'))),
  });

  if (app.release_state === 'released') return null;
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const v = name.trim().replace(/^@/, '');
    if (v) invite.mutate(v);
  };
  const full = (q.data?.length ?? 0) >= MAX_TESTERS;

  return (
    <section className="dev-card" aria-labelledby="dev-testers-h">
      <h2 id="dev-testers-h" className="dev-card-title">
        {t('developers.testers.heading')}
      </h2>
      <p className="dev-muted">{t('developers.testers.intro', { max: MAX_TESTERS })}</p>
      <form className="dev-actions" onSubmit={onSubmit}>
        <TextInput
          label={t('developers.testers.username')}
          placeholder={t('developers.testers.usernamePlaceholder')}
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={full}
        />
        <Button type="submit" loading={invite.isPending} disabled={!name.trim() || full}>
          {t('developers.testers.invite')}
        </Button>
      </form>
      {full && <p className="dev-muted">{t('developers.testers.full', { max: MAX_TESTERS })}</p>}
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      {q.isPending ? (
        <Loading what={t('developers.testers.testers')} />
      ) : q.error && isUnavailable(q.error) ? (
        <p className="dev-muted">{t('developers.testers.unavailable')}</p>
      ) : q.error ? (
        <LoadError
          what={t('developers.testers.testers')}
          error={q.error}
          onRetry={() => void q.refetch()}
        />
      ) : q.data.length === 0 ? (
        <p className="dev-muted">{t('developers.testers.none')}</p>
      ) : (
        <ul className="dev-uri-list" aria-label={t('developers.testers.listLabel')}>
          {q.data.map((tester) => {
            const label = tester.global_name ?? tester.username ?? tester.user_id;
            return (
              <li key={tester.id}>
                <span>
                  {label}{' '}
                  <span className="dev-muted">
                    {tester.state === 'accepted'
                      ? t('developers.testers.stateAccepted')
                      : t('developers.testers.stateInvited')}
                  </span>
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate(tester)}
                  aria-label={t('developers.testers.removeLabel', { name: label })}
                >
                  {tester.state === 'accepted'
                    ? t('developers.testers.remove')
                    : t('developers.testers.withdraw')}
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
