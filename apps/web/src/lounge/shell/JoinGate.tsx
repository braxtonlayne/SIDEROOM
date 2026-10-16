import { Button, Checkbox, Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { HOME } from '../channels';
import { useChannels, useCurrentUser, useGuild } from '../data';
import { useLoungeRoute } from '../nav';
import {
  acceptScreening,
  COMPLETED_ONBOARDING,
  completedOnboarding,
  getGuide,
  getMemberVerification,
  getOnboarding,
  getOwnMember,
  postOnboardingResponses,
  useGuideFlags,
  type MemberVerification,
  type Onboarding,
} from './joinPipeline';

/** How long after joining the onboarding questions are offered (members who joined earlier never saw them). */
const ONBOARDING_WINDOW_MS = 3 * 86_400_000;

type Step =
  { kind: 'screening'; form: MemberVerification } | { kind: 'onboarding'; form: Onboarding };

/**
 * What a new member goes through when they open a server (LNG-GLD-030, LNG-GLD-031): the rules they must agree to
 * (they stay read-only until they do), then the onboarding questions. Both are skippable here without side effects:
 * a pending member just stays read-only. The questions are offered to a member whose `COMPLETED_ONBOARDING` flag is
 * clear who joined within three days or has just accepted the rules (D-245); everyone else without the flag finds
 * "Channels & Roles" at the top of the channel list. It also notes whether the server has a guide so the server menu
 * can offer it (LNG-GLD-033).
 */
export function JoinGate() {
  const { guildId } = useLoungeRoute();
  const active = guildId && guildId !== HOME ? guildId : undefined;
  const me = useCurrentUser();
  const guild = useGuild(active);
  const [step, setStep] = useState<{ guildId: string; step: Step } | null>(null);
  const setGuide = useGuideFlags((s) => s.set);
  const setBrowse = useGuideFlags((s) => s.setOnboarding);
  const setFlags = useGuideFlags((s) => s.setMemberFlags);
  const owner = guild?.ownerId === me.id;
  const known = !!guild;

  useEffect(() => {
    if (!active || !known) return;
    let live = true;
    void (async () => {
      try {
        const guide = await getGuide(active).catch(() => null);
        if (live && guide) setGuide(active, guide.enabled);
        const form = await getOnboarding(active).catch(() => null);
        if (live && form) setBrowse(active, form.enabled);
        if (owner) return;
        const member = await getOwnMember(active);
        if (!live) return;
        setFlags(active, member.flags ?? 0);
        if (member.pending) {
          const rules = await getMemberVerification(active);
          if (live && rules.enabled)
            setStep({ guildId: active, step: { kind: 'screening', form: rules } });
          return;
        }
        if (completedOnboarding(member.flags) || useGuideFlags.getState().skipped[active]) return;
        const joined = member.joined_at ? Date.parse(member.joined_at) : 0;
        if (!joined || Date.now() - joined > ONBOARDING_WINDOW_MS) return;
        if (live && form?.enabled && form.prompts.some((p) => p.in_onboarding)) {
          setStep({ guildId: active, step: { kind: 'onboarding', form } });
        }
      } catch {
        /* the gate is a courtesy: a failed lookup leaves the server usable */
      }
    })();
    return () => {
      live = false;
    };
  }, [active, known, owner, me.id, setGuide, setBrowse, setFlags]);

  if (!active || !step || step.guildId !== active) return null;
  const close = () => setStep(null);
  if (step.step.kind === 'screening')
    return (
      <ScreeningDialog
        guildId={active}
        form={step.step.form}
        onClose={close}
        onAccepted={() => {
          // Offer the onboarding questions right after the rules.
          void getOnboarding(active).then((form) => {
            if (
              form.enabled &&
              form.prompts.some((p) => p.in_onboarding) &&
              !completedOnboarding(useGuideFlags.getState().memberFlags[active])
            )
              setStep({ guildId: active, step: { kind: 'onboarding', form } });
            else close();
          }, close);
        }}
      />
    );
  return <OnboardingDialog guildId={active} form={step.step.form} onClose={close} />;
}

/**
 * "Channels & Roles" (D-245): the onboarding questions, opened from the top of the channel list by a member who has
 * not answered them yet (whenever they joined).
 */
export function OnboardingEntryDialog({
  guildId,
  onClose,
}: {
  guildId: string;
  onClose: () => void;
}) {
  const [form, setForm] = useState<Onboarding | null>(null);
  useEffect(() => {
    let live = true;
    getOnboarding(guildId).then(
      (f) => live && (f.prompts.some((p) => p.in_onboarding) ? setForm(f) : onClose()),
      () => live && onClose(),
    );
    return () => {
      live = false;
    };
  }, [guildId, onClose]);
  return form ? <OnboardingDialog guildId={guildId} form={form} onClose={onClose} /> : null;
}

function ScreeningDialog({
  guildId,
  form,
  onClose,
  onAccepted,
}: {
  guildId: string;
  form: MemberVerification;
  onClose: () => void;
  onAccepted: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const [agreed, setAgreed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const rules = form.form_fields.flatMap((f) => f.values);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await acceptScreening(guildId);
      onAccepted();
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.joinGate.rulesError'));
      setBusy(false);
    }
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.joinGate.rulesTitle', { name: guild?.name ?? '' })}
      description={form.description ?? t('lounge.joinGate.rulesIntro')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.joinGate.notNow')}
          </Button>
          <Button disabled={!agreed || busy} onClick={() => void submit()}>
            {t('lounge.joinGate.rulesSubmit')}
          </Button>
        </>
      }
    >
      <ol className="lng-join-rules">
        {rules.map((r, i) => (
          <li key={i}>{r}</li>
        ))}
      </ol>
      <Checkbox
        label={t('lounge.joinGate.rulesAgree')}
        checked={agreed}
        onChange={(e) => setAgreed(e.target.checked)}
      />
      {error && (
        <p role="alert" className="lng-join-error">
          {error}
        </p>
      )}
    </Modal>
  );
}

function OnboardingDialog({
  guildId,
  form,
  onClose,
}: {
  guildId: string;
  form: Onboarding;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const prompts = form.prompts.filter((p) => p.in_onboarding);
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string[] | null>(null);

  const idOf = (promptIdx: number, optIdx: number) =>
    prompts[promptIdx]!.options[optIdx]!.id ?? `${promptIdx}:${optIdx}`;
  const toggle = (pi: number, oi: number) => {
    const id = idOf(pi, oi);
    const p = prompts[pi]!;
    setPicked((cur) => {
      if (cur.includes(id)) return cur.filter((x) => x !== id);
      const own = p.options.map((_, k) => idOf(pi, k));
      return [...(p.single_select ? cur.filter((x) => !own.includes(x)) : cur), id];
    });
  };
  const missing = prompts.some(
    (p, pi) => p.required && !p.options.some((_, oi) => picked.includes(idOf(pi, oi))),
  );

  const skip = () => {
    useGuideFlags.getState().skip(guildId);
    onClose();
  };
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const out = await postOnboardingResponses(guildId, picked);
      // The server set COMPLETED_ONBOARDING; the channel list stops offering "Channels & Roles".
      const { memberFlags, setMemberFlags } = useGuideFlags.getState();
      setMemberFlags(guildId, (memberFlags[guildId] ?? 0) | COMPLETED_ONBOARDING);
      setResult(out.channel_ids);
      setBusy(false);
    } catch (e) {
      setError(
        e instanceof ApiError && e.message ? e.message : t('lounge.joinGate.onboardingError'),
      );
      setBusy(false);
    }
  };

  if (result)
    return (
      <Modal
        open
        onClose={onClose}
        size="sm"
        title={t('lounge.joinGate.browseTitle')}
        description={t('lounge.joinGate.browseIntro', { name: guild?.name ?? '' })}
        footer={<Button onClick={onClose}>{t('lounge.joinGate.done')}</Button>}
      >
        <ul className="lng-join-rules" aria-label={t('lounge.joinGate.browseTitle')}>
          {result.map((id) => (
            <li key={id}>#{channels.find((c) => c.id === id)?.name ?? id}</li>
          ))}
        </ul>
      </Modal>
    );

  return (
    <Modal
      open
      onClose={skip}
      size="md"
      title={t('lounge.joinGate.onboardingTitle', { name: guild?.name ?? '' })}
      description={t('lounge.joinGate.onboardingIntro')}
      footer={
        <>
          <Button variant="ghost" onClick={skip}>
            {t('lounge.joinGate.skip')}
          </Button>
          <Button disabled={missing || busy} onClick={() => void submit()}>
            {t('lounge.joinGate.onboardingSubmit')}
          </Button>
        </>
      }
    >
      {prompts.map((p, pi) => (
        <fieldset key={p.id ?? pi} className="lng-join-prompt">
          <legend>
            {p.title}
            {p.required ? ` ${t('lounge.joinGate.required')}` : ''}
          </legend>
          {p.options.map((o, oi) => (
            <Checkbox
              key={o.id ?? oi}
              label={`${o.emoji ? `${o.emoji} ` : ''}${o.title}`}
              {...(o.description ? { description: o.description } : {})}
              checked={picked.includes(idOf(pi, oi))}
              onChange={() => toggle(pi, oi)}
            />
          ))}
        </fieldset>
      ))}
      {error && (
        <p role="alert" className="lng-join-error">
          {error}
        </p>
      )}
    </Modal>
  );
}
