// LNG-SSET-065: preview of the onboarding flow as a new member sees it. It works on the unsaved draft, so an
// admin can try the answers and see which channels and roles each combination hands out before saving.
import { Button, Checkbox, Modal } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { useT } from '../../../i18n';
import type * as api from '../api';

interface Named {
  id: string;
  name?: string | null;
}

export function OnboardingPreview({
  prompts,
  defaultChannelIds,
  channels,
  roles,
}: {
  prompts: api.OnboardingPrompt[];
  defaultChannelIds: string[];
  channels: Named[];
  roles: Named[];
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="secondary" onClick={() => setOpen(true)}>
        {t('settings.onboardingPreview.open')}
      </Button>
      {open && (
        <Modal
          open
          onClose={() => setOpen(false)}
          size="md"
          title={t('settings.onboardingPreview.title')}
          description={t('settings.onboardingPreview.description')}
        >
          <PreviewBody
            prompts={prompts}
            defaultChannelIds={defaultChannelIds}
            channels={channels}
            roles={roles}
          />
        </Modal>
      )}
    </>
  );
}

function PreviewBody({
  prompts,
  defaultChannelIds,
  channels,
  roles,
}: {
  prompts: api.OnboardingPrompt[];
  defaultChannelIds: string[];
  channels: Named[];
  roles: Named[];
}) {
  const t = useT();
  /** Chosen option indexes per question index. */
  const [picked, setPicked] = useState<Record<number, number[]>>({});
  const shown = prompts.map((p, i) => ({ p, i })).filter(({ p }) => p.in_onboarding);

  const choose = (qi: number, oi: number, on: boolean, single: boolean) =>
    setPicked((cur) => {
      const now = cur[qi] ?? [];
      const next = single ? (on ? [oi] : []) : on ? [...now, oi] : now.filter((x) => x !== oi);
      return { ...cur, [qi]: next };
    });

  const result = useMemo(() => {
    const ch = new Set(defaultChannelIds);
    const ro = new Set<string>();
    for (const [qi, opts] of Object.entries(picked)) {
      const p = prompts[Number(qi)];
      for (const oi of opts) {
        const o = p?.options[oi];
        o?.channel_ids.forEach((c) => ch.add(c));
        o?.role_ids.forEach((r) => ro.add(r));
      }
    }
    const name = (list: Named[], id: string) => list.find((x) => x.id === id)?.name ?? id;
    return {
      channels: [...ch].map((c) => `#${name(channels, c)}`),
      roles: [...ro].map((r) => name(roles, r)),
    };
  }, [picked, prompts, defaultChannelIds, channels, roles]);

  const missing = shown.some(({ p, i }) => p.required && (picked[i] ?? []).length === 0);

  return (
    <div className="ss-form">
      {shown.length === 0 && <p className="ss-na">{t('settings.onboardingPreview.noQuestions')}</p>}
      {shown.map(({ p, i }) => (
        <fieldset key={i} className="ss-card">
          <legend>
            {p.title || t('settings.community.questionN', { n: i + 1 })}
            {p.required ? ` ${t('settings.onboardingPreview.required')}` : ''}
          </legend>
          <p className="ss-sub">
            {p.single_select
              ? t('settings.onboardingPreview.pickOne')
              : t('settings.onboardingPreview.pickMany')}
          </p>
          {p.options.map((o, oi) => (
            <Checkbox
              key={oi}
              label={`${o.emoji ? `${o.emoji} ` : ''}${o.title || t('settings.onboardingPreview.untitled')}`}
              {...(o.description ? { description: o.description } : {})}
              checked={(picked[i] ?? []).includes(oi)}
              onChange={(e) => choose(i, oi, e.target.checked, p.single_select)}
            />
          ))}
        </fieldset>
      ))}
      {missing && <p className="ss-sub">{t('settings.onboardingPreview.answerRequired')}</p>}
      <section className="ss-card" aria-label={t('settings.onboardingPreview.result')}>
        <h3>{t('settings.onboardingPreview.result')}</h3>
        <p>
          <strong>{t('settings.onboardingPreview.channels')}</strong>{' '}
          {result.channels.length > 0
            ? result.channels.join(', ')
            : t('settings.onboardingPreview.nothing')}
        </p>
        <p>
          <strong>{t('settings.onboardingPreview.roles')}</strong>{' '}
          {result.roles.length > 0
            ? result.roles.join(', ')
            : t('settings.onboardingPreview.nothing')}
        </p>
      </section>
    </div>
  );
}
